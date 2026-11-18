# Feature parity

Before its first release, Singline must offer every feature of the native macOS app (Linory 0.3.1, contributed in [#1](https://github.com/JulianRudrich/LyricsOverlay/pull/1)) on every system. Before that release, this list is compared again with the latest version of Linory.

**Legend:** ✅ built and tested · 🧪 built, not yet tested on a device · ⬜ open · — not available on this system

**Columns:** Lin = Linux, Win = Windows, Mac = macOS. Linux is tested automatically on a virtual display with a simulated player.

## Overlay

| Feature | Lin | Win | Mac | Notes |
|---|:-:|:-:|:-:|---|
| Floats above other windows on every desktop | ✅ | 🧪 | 🧪 | macOS above full-screen apps: measurement A |
| Never takes the focus from other apps | 🧪 | 🧪 | 🧪 | |
| Dragging, four corners, remembered position, several displays | 🧪 | 🧪 | 🧪 | |
| Click-through, remembered; opening the app again turns it off | 🧪 | 🧪 | 🧪 | |
| Shortcut to show and hide | 🧪 | 🧪 | 🧪 | Linux: X11 only for now |
| Three layouts, font size, opacity and size | ✅ | 🧪 | 🧪 | Linory has one fixed layout |
| Word-by-word highlight from real word timing | ✅ | 🧪 | 🧪 | Linory mostly estimates |
| Estimated word highlight for line-synced lyrics | ✅ | 🧪 | 🧪 | |
| Scrolling lines and smooth song changes | ✅ | 🧪 | 🧪 | |
| Splitting long lines | ✅ | 🧪 | 🧪 | |
| Cover art | 🧪 | 🧪 | 🧪 | Windows: thumbnail of the media session; macOS: Spotify only so far |
| Hiding while nothing plays | 🧪 | 🧪 | 🧪 | |

## Mini-player

| Feature | Lin | Win | Mac | Notes |
|---|:-:|:-:|:-:|---|
| Play/pause, next, previous and seeking (commands) | ✅ | 🧪 | 🧪 | |
| Controls and a seek bar with times | ✅ | 🧪 | 🧪 | macOS: hover while the app is inactive needs checking |
| Jumping to a line by clicking it | ✅ | 🧪 | 🧪 | |
| Chorus markers | ✅ | 🧪 | 🧪 | |
| ♥ (Spotify) | ⬜ | ⬜ | ⬜ | Needs the Spotify connection |
| Open in Spotify | ⬜ | ⬜ | ⬜ | |

## Sync and lyrics

| Feature | Lin | Win | Mac | Notes |
|---|:-:|:-:|:-:|---|
| Position carried forward between readings, jitter smoothed | ✅ | 🧪 | 🧪 | Lines change about 10 ms after their timestamp |
| Immediate reaction to song changes through system notifications | ⬜ | ⬜ | ⬜ | Today the players are read every 0.5 s (macOS 0.8 s) |
| Timing correction per song and globally | ✅ | 🧪 | 🧪 | Linory: global only |
| LRCLIB exact match, then a search that prefers the same album | ✅ | ✅ | ✅ | Tested independently of the system |
| Local cache | ✅ | ✅ | ✅ | |
| Plain lyrics, marked as not synced | ✅ | ✅ | ✅ | Timing follows the line length instead of even spacing |
| Instrumentals recognised | ✅ | ✅ | ✅ | |
| Own lyrics: storage | ✅ | ✅ | ✅ | |
| Own lyrics: LRC import and editor | ✅ | 🧪 | 🧪 | From the 0.2 preview |

## Players

| Player | Lin | Win | Mac | Notes |
|---|:-:|:-:|:-:|---|
| Spotify (desktop app) | ✅ | 🧪 | 🧪 | Linux tested with a simulated player |
| Apple Music | 🧪 | 🧪 | 🧪 | Windows: the combined "Artist — Album" field is split; Linux: web player |
| Amazon Music | 🧪 | 🧪 | ⬜ | Windows: measurement B; Linux: web player; macOS: browser extension |
| Automatic or fixed source | ✅ | ✅ | ✅ | |
| Other players (browsers, VLC and more) | ✅ | 🧪 | ⬜ | macOS: browser extension |

## More features

| Feature | Lin | Win | Mac | Notes |
|---|:-:|:-:|:-:|---|
| English and German, following the system language | ✅ | 🧪 | 🧪 | |
| Open at login | 🧪 | 🧪 | 🧪 | |
| Tray menu | 🧪 | 🧪 | 🧪 | Linory: glass panel with the current song (planned) |
| Sleep timer | 🧪 | 🧪 | 🧪 | |
| Listening statistics | ✅ | 🧪 | 🧪 | For every player |
| Spotify connection, True Shuffle and top lists | ⬜ | ⬜ | ⬜ | In progress |
| Installers | ⬜ | 🧪 | 🧪 | CI builds .exe, .msi and .dmg |
| Homebrew | — | — | ⬜ | |
| Automatic updates | ⬜ | ⬜ | ⬜ | |

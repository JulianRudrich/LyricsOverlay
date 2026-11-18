# Measurements

Three questions whose answers shape the next milestones. The steps for A and B are in [TESTING.md](TESTING.md).

## A: Overlay behaviour on each system

**Question:** does the overlay stay above every window, including full-screen apps, on every desktop, without taking the focus, and with working click-through?

**Known so far:**

- Linux on X11: works in the automated test. Without a compositor (as on the virtual display) the rounded corners are black instead of transparent; on real desktops they are not.
- Linux on Wayland: the compositor does not keep normal windows on top. GNOME needs XWayland; KDE and wlroots-based desktops support the layer shell ([tauri#3117](https://github.com/tauri-apps/tauri/issues/3117)).
- macOS: staying above full-screen apps needs the `fullScreenAuxiliary` window behaviour, which Tauri does not offer directly. The Swift app has it.

**Result:** open

## B: What the players report

**Question:** do Spotify, Apple Music and Amazon Music on Windows and macOS report title, artist, album and position, and accept commands?

**Known from research:**

- Windows reports the position only as of its last change; Singline moves it forward from there. Spotify updates it only on play, pause and seek.
- Apple Music for Windows puts "Artist — Album" into the artist field and leaves the album empty; Singline splits it ([report](https://github.com/marcimastro98/Xenon/issues/128)).
- According to another project, Amazon Music for Windows reports only part of the data ([bridge](https://github.com/GoldenPigeon92/Amazon-Music-SMTC-Bridge)).
- Amazon Music for macOS has no AppleScript interface. Other players on macOS can only be reached through an unofficial system interface ([mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)).

**Result:**

| Player | Windows | macOS |
|---|---|---|
| Spotify | open | open |
| Apple Music | open | open |
| Amazon Music | open | open |
| Browser (YouTube Music, Amazon Music on the web) | open | — |

## C: Word timing on LRCLIB

**Question:** does LRCLIB provide word timing (a `lyricsfile` with `words`), and for how many songs?

**Result (17 November 2026, ten well-known songs, 20 search results each):**

- Every result has a `lyricsfile` field (YAML as text) and a `hasWordSync` flag. All 200 documents could be read.
- **88 of 200** results have word timing; **8 of 10 songs** have at least one version with word timing.
- The share varies widely: "As It Was" 19/20, "Levitating" 17/20, "Haus am See" 17/20, "Blinding Lights" 16/20, "Sonne" 13/20, "Espresso" 4/20, "BIRDS OF A FEATHER" 1/20, "Shape of You" 1/20, "Cruel Summer" 0/20, "Bohemian Rhapsody" 0/20.

What Singline's lookup finds on the real LRCLIB (`cargo run -p lyrics-providers --example lookup`):

| Song | Result |
|---|---|
| The Weeknd – Blinding Lights | Word by word, 35 lines |
| Harry Styles – As It Was | Word by word, 37 lines |
| Rammstein – Sonne | Word by word, 40 lines |
| Peter Fox – Haus am See (216 s) | Word by word, 50 lines |
| Taylor Swift – Cruel Summer | Line by line (no word timing available) |
| Queen – Bohemian Rhapsody | Line by line (no word timing available) |

**Consequences:**

- Real word-by-word highlighting is available right away for a large share of well-known songs. The rest needs the estimated highlight.
- An exact match (duration within ±2.5 s) always wins, even without word timing, because a version with a different duration often has shifted timing. Example: "Haus am See" at 222 s matches an exact version without word timing; the versions with word timing are 216 s long.
- On a tie in the search, word timing now wins; this showed up in the test with real data and is fixed.
- Idea for later: if the exact match has no word timing, take a version with word timing and almost the same duration (±1 s) from the search.

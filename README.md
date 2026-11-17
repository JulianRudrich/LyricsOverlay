# Lyrics Overlay

Synced lyrics in a small window above your other apps, for Windows, macOS and Linux, with Spotify, Apple Music and Amazon Music.

> **Status: early development (phase 0).** Nothing on this branch is released yet. The released app is the macOS-only Swift version on [`main`](https://github.com/JulianRudrich/LyricsOverlay/tree/main). This branch, `next`, is its cross-platform successor.

## What works today

- **Follows your player.** Linux through MPRIS (Spotify and browser players), Windows through the system media controls, macOS through AppleScript for Spotify and Music. A player that is not running is never started.
- **Finds the lyrics.** Your own lyrics first, then a local cache, then [LRCLIB](https://lrclib.net): the exact recording, then a search that prefers the same album and word timing. Plain lyrics are spread over the song and marked as not synced.
- **Highlights word by word** when the lyrics have word timing, and line by line otherwise. The position runs on smoothly between two readings of the player.
- **An overlay you can adjust:** one line, focus or context layout, font size, opacity, size, a corner, click-through, a timing correction per song and overall, and a shortcut (Alt+Shift+L) to show or hide it.
- **German and English**, following the system language.

The [roadmap](docs/ROADMAP.md) lists what comes next; [PARITY.md](docs/PARITY.md) tracks every feature against the macOS forks.

## Try it

You need Rust 1.85 or later, Node.js 22 and pnpm 10. On Linux, also the WebKitGTK development files:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
```

Then:

```bash
pnpm install
pnpm --filter lyrics-overlay-desktop tauri build --debug --no-bundle
./target/debug/lyrics-overlay --demo
```

`--demo` plays a built-in song, so you can see the overlay without a player. Leave it out to follow your music.

## See what your players report

`lyrics-overlay-watch` prints, live, what every player on the computer reports: title, artist, album, position, how far the position drifts, and which controls the player offers.

```bash
cargo run --release -p media-session --bin lyrics-overlay-watch
```

## Repository

| Path | What it is |
|---|---|
| `crates/lyrics-core` | Pure logic: LRC and Lyricsfile parsing, matching, the playback clock, choosing a player |
| `crates/media-session` | Reading and controlling players on each system, plus `lyrics-overlay-watch` |
| `crates/lyrics-providers` | LRCLIB client and the SQLite store for your own lyrics, corrections and the cache |
| `apps/desktop` | The Tauri app: Rust in `src-tauri`, the Svelte interface in `src` |
| `docs` | Roadmap, feature parity, test instructions and measurements |

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --filter lyrics-overlay-desktop check
pnpm --filter lyrics-overlay-desktop test
bash apps/desktop/e2e/linux-smoke.sh
```

`cargo test --workspace` needs the built interface (`pnpm --filter lyrics-overlay-desktop build`); plain `cargo test` covers the crates only. The end-to-end check starts the app on a virtual display with a pretend Spotify and a pretend LRCLIB and checks every line the overlay shows against the playback time.

## License

GPL-3.0-or-later, see [LICENSE](LICENSE). The lyric logic started as a port of the MIT-licensed Swift app by the same author. Song lyrics belong to their rights holders. Not affiliated with Spotify, Apple, Amazon or LRCLIB.

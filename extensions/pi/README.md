# Singline for pi

Shows the line being sung in the footer of [pi](https://github.com/earendil-works/pi), the coding agent in your terminal, while Singline plays a song. Words light up as they are sung. Paused songs show their line dimmed, and between lines it shows the song and artist.

The extension reads `now.json`, which the Singline app keeps up to date on your computer. Singline has to be running; nothing leaves your computer. It works in any terminal pi runs in, such as Kitty, WezTerm, Ghostty or the system's own.

## Install

```bash
mkdir -p ~/.pi/agent/extensions
```

```bash
curl -fsSL https://raw.githubusercontent.com/JulianRudrich/Singline/main/extensions/pi/singline.ts -o ~/.pi/agent/extensions/singline.ts
```

Start pi, or run `/reload` in a running pi. To try it once without installing, start pi with `pi -e ./extensions/pi/singline.ts` from a checkout of this repository.

## Use

- `/singline` shows or hides the lyrics.
- The line is cut after 90 characters, so the footer keeps room for pi's own information.

## Where `now.json` lives

| System | Path |
|---|---|
| macOS | `~/Library/Caches/io.github.julianrudrich.singline/now.json` |
| Linux | `~/.cache/io.github.julianrudrich.singline/now.json` (or under `$XDG_CACHE_HOME`) |
| Windows | `%LOCALAPPDATA%\io.github.julianrudrich.singline\now.json` |

`SINGLINE_NOW_FILE` points the extension at another file.

## Development

The extension is one TypeScript file without dependencies; pi loads it as it is. The tests run with Node.js 22 or later:

```bash
node --test extensions/pi/test/*.test.ts
```

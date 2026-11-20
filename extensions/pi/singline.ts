/**
 * Singline for pi: the line being sung shows in pi's footer, word by word,
 * while Singline plays a song. Singline writes what plays to `now.json` on
 * your computer; this extension reads it four times a second. Nothing leaves
 * your computer.
 *
 * Install: copy this file to ~/.pi/agent/extensions/singline.ts and start pi.
 * `/singline` shows or hides the lyrics.
 */
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const IDENTIFIER = "io.github.julianrudrich.singline";
/** The format of now.json this extension reads. */
const VERSION = 1;
/** Singline rewrites the file every few seconds; an older one means it quit. */
const STALE_MS = 20_000;
const TICK_MS = 250;
/** Longer lines are cut, so the footer keeps its other information. */
const MAX_LENGTH = 90;

export interface Word {
  start: number;
  end?: number;
  text: string;
}

export interface Line {
  time: number;
  end?: number;
  text: string;
  words?: Word[];
}

/** What Singline writes to now.json. Times are seconds, moments milliseconds since 1970. */
export interface Now {
  version: number;
  updatedAt: number;
  player: string | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration: number | null;
  playing: boolean;
  position: number | null;
  at: number;
  rate: number;
  offset: number;
  lyrics: { lines: Line[]; sync: "word" | "line" | "unsynced"; instrumental: boolean } | null;
}

/** The song, and the line being sung split where the singing is. */
export interface Moment {
  title: string;
  artist: string;
  playing: boolean;
  sung: string;
  rest: string;
}

/** Where Singline keeps now.json on this system. */
export function nowFile(env: NodeJS.ProcessEnv = process.env, platform: string = process.platform, home: string = homedir()): string {
  if (env.SINGLINE_NOW_FILE) return env.SINGLINE_NOW_FILE;
  if (env.SINGLINE_DATA_DIR) return join(env.SINGLINE_DATA_DIR, "now.json");
  if (platform === "darwin") return join(home, "Library", "Caches", IDENTIFIER, "now.json");
  if (platform === "win32") return join(env.LOCALAPPDATA ?? join(home, "AppData", "Local"), IDENTIFIER, "now.json");
  const cache = env.XDG_CACHE_HOME?.startsWith("/") ? env.XDG_CACHE_HOME : join(home, ".cache");
  return join(cache, IDENTIFIER, "now.json");
}

/** The position at `at`: moved on while the song plays, never past its end. */
export function positionAt(now: Now, at: number): number | null {
  if (now.position === null) return null;
  const elapsed = now.playing ? (Math.max(0, at - now.at) / 1000) * now.rate : 0;
  const moved = Math.max(0, now.position + elapsed);
  return now.duration && now.duration > 0 ? Math.min(moved, now.duration) : moved;
}

/** The last line that has started by `time`, as in Singline itself. */
export function currentIndex(lines: Line[], time: number): number | null {
  let lower = 0;
  let upper = lines.length;
  while (lower < upper) {
    const middle = (lower + upper) >> 1;
    if (lines[middle].time <= time) lower = middle + 1;
    else upper = middle;
  }
  return lower === 0 ? null : lower - 1;
}

/** Splits a line at `time`: by its words where they are timed, else by how far the line has come. */
export function split(line: Line, nextTime: number | undefined, time: number): { sung: string; rest: string } {
  const text = line.text.trim();
  if (line.words && line.words.length > 0) {
    const sung = line.words.filter((word) => word.start <= time).map((word) => word.text).join("");
    const rest = line.words.filter((word) => word.start > time).map((word) => word.text).join("");
    return { sung: sung.trimStart(), rest: sung.trim() ? rest.trimEnd() : rest.trim() };
  }
  const end = line.end ?? nextTime ?? line.time + 4;
  const progress = end > line.time ? Math.min(1, Math.max(0, (time - line.time) / (end - line.time))) : 1;
  // Cut after the last whole word reached, so words light up one by one.
  const reached = Math.round(text.length * progress);
  const cut = progress >= 1 ? text.length : text.lastIndexOf(" ", reached) + 1;
  return { sung: text.slice(0, cut), rest: text.slice(cut) };
}

/** What plays at `at`, or null for no song or a Singline that has quit. */
export function momentAt(now: Now, at: number): Moment | null {
  if (now.version !== VERSION || at - now.updatedAt > STALE_MS || !now.title) return null;
  const moment: Moment = { title: now.title, artist: now.artist ?? "", playing: now.playing, sung: "", rest: "" };
  const position = positionAt(now, at);
  const lyrics = now.lyrics;
  if (position === null || !lyrics || lyrics.sync === "unsynced") return moment;
  const time = position + now.offset;
  const index = currentIndex(lyrics.lines, time);
  if (index === null || !lyrics.lines[index].text.trim()) return moment;
  return { ...moment, ...split(lyrics.lines[index], lyrics.lines[index + 1]?.time, time) };
}

/** Shortens text to `max` characters, ending with an ellipsis. */
export function shorten(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 1).trimEnd()}…`;
}

type Theme = ExtensionContext["ui"]["theme"];

function footer(moment: Moment, theme: Theme): string {
  const note = theme.fg("dim", "♪ ");
  if (!moment.sung && !moment.rest) return note + theme.fg("dim", shorten(`${moment.title} – ${moment.artist}`, MAX_LENGTH));
  const sung = shorten(moment.sung, MAX_LENGTH);
  const rest = shorten(moment.rest, Math.max(0, MAX_LENGTH - sung.length));
  if (!moment.playing) return note + theme.fg("dim", sung + rest);
  return note + theme.bold(sung) + theme.fg("dim", rest);
}

export default function singline(pi: ExtensionAPI) {
  const file = nowFile();
  let timer: ReturnType<typeof setInterval> | undefined;
  let visible = true;
  let shown: string | undefined;
  let read = { mtime: -1, now: null as Now | null };

  // Reads now.json again only after Singline rewrote it.
  function current(): Now | null {
    try {
      const mtime = statSync(file).mtimeMs;
      if (mtime !== read.mtime) read = { mtime, now: JSON.parse(readFileSync(file, "utf8")) as Now };
    } catch {
      read = { mtime: -1, now: null };
    }
    return read.now;
  }

  function show(ctx: ExtensionContext) {
    const now = visible ? current() : null;
    const moment = now ? momentAt(now, Date.now()) : null;
    const text = moment ? footer(moment, ctx.ui.theme) : undefined;
    if (text === shown) return;
    shown = text;
    ctx.ui.setStatus("singline", text);
  }

  pi.on("session_start", async (_event, ctx) => {
    if (!ctx.hasUI) return;
    clearInterval(timer);
    timer = setInterval(() => show(ctx), TICK_MS);
    show(ctx);
  });

  pi.on("session_shutdown", async () => {
    clearInterval(timer);
    timer = undefined;
  });

  pi.registerCommand("singline", {
    description: "Show or hide the lyrics Singline is playing",
    handler: async (_args, ctx) => {
      visible = !visible;
      show(ctx);
      ctx.ui.notify(visible ? "Singline: lyrics shown" : "Singline: lyrics hidden", "info");
    },
  });
}

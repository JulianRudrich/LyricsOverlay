import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo, Corner, LyricsView, Playback, Settings, Snapshot } from "./types";

export type PlayerCommand =
  | { kind: "playPause" }
  | { kind: "next" }
  | { kind: "previous" }
  | { kind: "seekTo"; position: number };

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("update_settings", { patch }),
  adjustSongOffset: (delta: number) => invoke<number>("adjust_song_offset", { delta }),
  setSongOffset: (value: number) => invoke<number>("set_song_offset", { value }),
  reloadLyrics: () => invoke<void>("reload_lyrics"),
  playerCommand: (command: PlayerCommand) => invoke<void>("player_command", { command }),
  openSettings: () => invoke<void>("open_settings"),
  moveOverlay: (corner: Corner) => invoke<void>("move_overlay", { corner }),
  setLaunchAtLogin: (enabled: boolean) => invoke<boolean>("set_launch_at_login", { enabled }),
  reportLine: (position: number, text: string) => invoke<void>("report_line", { position, text }),
};

export interface Listeners {
  playback?: (playback: Playback) => void;
  lyrics?: (lyrics: LyricsView) => void;
  settings?: (settings: Settings) => void;
  app?: (app: AppInfo) => void;
}

/** Subscribes to the events the Rust side sends; returns one function to stop. */
export async function subscribe(listeners: Listeners): Promise<UnlistenFn> {
  const stops = await Promise.all([
    listeners.playback && listen<Playback>("playback", (event) => listeners.playback?.(event.payload)),
    listeners.lyrics && listen<LyricsView>("lyrics", (event) => listeners.lyrics?.(event.payload)),
    listeners.settings && listen<Settings>("settings", (event) => listeners.settings?.(event.payload)),
    listeners.app && listen<AppInfo>("app", (event) => listeners.app?.(event.payload)),
  ]);
  return () => stops.forEach((stop) => stop?.());
}

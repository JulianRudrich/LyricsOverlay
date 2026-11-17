// Mirrors the JSON the Rust side sends. Field names follow serde's camelCase.

export type Service = "spotify" | "appleMusic" | "amazonMusic" | "other";
export type SourcePreference = "automatic" | "spotify" | "appleMusic" | "amazonMusic";
export type Layout = "caption" | "focus" | "context";
export type Language = "system" | "de" | "en";
export type Corner = "topLeft" | "topRight" | "bottomLeft" | "bottomRight";

export interface PlayerInfo {
  id: string;
  name: string;
  service: Service;
  web: boolean;
}

export interface Capabilities {
  playPause: boolean;
  next: boolean;
  previous: boolean;
  seek: boolean;
}

export interface Track {
  key: string;
  sourceId: string | null;
  title: string;
  artist: string;
  album: string | null;
  duration: number | null;
  artworkUrl: string | null;
}

export type Problem =
  | { kind: "permissionDenied" }
  | { kind: "noTrackData" }
  | { kind: "readFailed"; detail: string };

export interface Playback {
  state: "idle" | "problem" | "track";
  player: PlayerInfo | null;
  problem: Problem | null;
  track: Track | null;
  playing: boolean;
  /** Seconds into the track at the moment the Rust side sent this. */
  position: number | null;
  rate: number;
  capabilities: Capabilities;
}

export interface LyricWord {
  start: number;
  end?: number;
  text: string;
}

export interface LyricLine {
  time: number;
  end?: number;
  text: string;
  words?: LyricWord[];
}

export type SyncLevel = "word" | "line" | "unsynced";

export interface Lyrics {
  lines: LyricLine[];
  sync: SyncLevel;
  instrumental: boolean;
}

export type LyricsStatus =
  | "idle"
  | "loading"
  | "ready"
  | "instrumental"
  | "notFound"
  | "localOnly"
  | "offline"
  | "rateLimited"
  | "failed";

export interface LyricsView {
  trackKey: string | null;
  status: LyricsStatus;
  origin: "local" | "online" | "demo" | null;
  lyrics: Lyrics | null;
  songOffset: number;
  canRetry: boolean;
}

export interface Settings {
  layout: Layout;
  fontSize: number;
  backgroundOpacity: number;
  overlayOpacity: number;
  width: number;
  height: number;
  globalOffset: number;
  lookupOnline: boolean;
  source: SourcePreference;
  includeOtherPlayers: boolean;
  clickThrough: boolean;
  language: Language;
  overlayVisible: boolean;
  hotkey: string;
}

export interface AppInfo {
  version: string;
  demo: boolean;
  /** Reports each shown line to the Rust side, for end-to-end checks. */
  debug: boolean;
  platform: "macos" | "windows" | "linux" | "other";
  /** The language the interface uses after resolving "system". */
  language: "de" | "en";
  hotkeyError: boolean;
  launchAtLogin: boolean;
}

export interface Snapshot {
  settings: Settings;
  playback: Playback;
  lyrics: LyricsView;
  app: AppInfo;
}

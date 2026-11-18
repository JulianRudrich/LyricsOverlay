// Mirrors the JSON the Rust side sends. Field names follow serde's camelCase.

export type Service = "spotify" | "appleMusic" | "amazonMusic" | "youTubeMusic" | "other";
export type SourcePreference = "automatic" | "spotify" | "appleMusic" | "amazonMusic" | "youTubeMusic";
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
  /** Lyric times where each chorus starts; empty when nothing repeats or the lyrics are not synced. */
  chorus: number[];
  songOffset: number;
  canRetry: boolean;
}

/** The song the lyrics editor belongs to, fixed when its window opens. */
export interface EditorState {
  trackKey: string;
  title: string;
  artist: string;
  /** Your own lyrics as LRC text, null before you add some. */
  lrc: string | null;
}

/** Replaces `length` characters at `start`; all counts are UTF-16 code units. */
export interface StampEdit {
  start: number;
  length: number;
  replacement: string;
  nextCaret: number;
}

/** Why the Rust side did not stamp a line. */
export type StampError = "otherTrack" | "noPosition" | "notALyricLine";

/** Why own lyrics could not be opened, imported, saved or removed. */
export type OwnLyricsError = "noTrack" | "tooLarge" | "notText" | "missingTimestamps" | "unreadable" | "storage";

export interface Settings {
  layout: Layout;
  fontSize: number;
  backgroundOpacity: number;
  overlayOpacity: number;
  width: number;
  height: number;
  globalOffset: number;
  lookupOnline: boolean;
  keepHistory: boolean;
  source: SourcePreference;
  includeOtherPlayers: boolean;
  clickThrough: boolean;
  language: Language;
  overlayVisible: boolean;
  autoHide: boolean;
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

/** A browser that paired with Singline through the extension. */
export interface PairedBrowser {
  id: string;
  name: string;
  /** Seconds since 1970. */
  pairedAt: number;
  connected: boolean;
  /** Tabs it reports right now. */
  tabs: number;
}

/** The connection to the Singline browser extension, as the settings show it. */
export interface BridgeView {
  /** False in the demo. */
  available: boolean;
  port: number | null;
  error: string | null;
  /** Six digits to enter in the extension; valid while the settings show it. */
  pairingCode: string | null;
  browsers: PairedBrowser[];
}

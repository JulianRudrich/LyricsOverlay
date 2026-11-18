// True Shuffle as the Rust side reports it (serde camelCase), the commands
// of the shuffle window and the texts it builds.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { formatNumber, type MessageKey, type Translate } from "../lib/i18n";

export const LIKED_SONGS = "liked";

export interface ShuffleSource {
  id: string;
  /** `null` for Liked Songs. */
  name: string | null;
}

export type ShuffleActivity =
  | { kind: "loading"; loaded: number; total: number }
  | { kind: "writing"; written: number; total: number }
  | { kind: "starting" }
  | { kind: "working" }
  | { kind: "continuing" };

export type ShuffleNotice =
  | { kind: "empty" }
  | { kind: "full" }
  | { kind: "noDevice" }
  | { kind: "kept"; name: string }
  | { kind: "auth" }
  | { kind: "offline" }
  | { kind: "premium" }
  | { kind: "http"; status: number }
  | { kind: "failed"; detail: string };

export interface UpNext {
  uri: string;
  name: string;
  artist: string;
}

export interface ShuffleView {
  connected: boolean;
  sources: ShuffleSource[];
  sourcesLoaded: boolean;
  mixes: string[];
  selected: string;
  members: string[];
  played: number;
  total: number;
  removed: number;
  keepPlaying: boolean;
  activity: ShuffleActivity | null;
  notice: ShuffleNotice | null;
  hasShuffle: boolean;
  activeSource: string | null;
  inShuffle: boolean;
  upNext: UpNext[];
}

export const shuffle = {
  openWindow: () => invoke<void>("shuffle_open_window"),
  view: () => invoke<ShuffleView>("shuffle_view"),
  loadSources: () => invoke<ShuffleView>("shuffle_load_sources"),
  select: (source: string) => invoke<ShuffleView>("shuffle_select", { source }),
  toggleSource: (source: string) => invoke<ShuffleView>("shuffle_toggle_source", { source }),
  play: () => invoke<ShuffleView>("shuffle_play"),
  reset: () => invoke<ShuffleView>("shuffle_reset"),
  restoreRemoved: () => invoke<ShuffleView>("shuffle_restore_removed"),
  setKeepPlaying: (on: boolean) => invoke<ShuffleView>("shuffle_set_keep_playing", { on }),
  removeCurrent: () => invoke<ShuffleView>("shuffle_remove_current"),
  keep: (name: string) => invoke<ShuffleView>("shuffle_keep", { name }),
};

export function onShuffle(callback: (view: ShuffleView) => void): Promise<UnlistenFn> {
  return listen<ShuffleView>("shuffle", (event) => callback(event.payload));
}

/** The sources of a mix ID, or the source itself. */
export function mixMembers(id: string): string[] {
  return id.startsWith("mix:") ? id.slice(4).split(",").filter(Boolean) : [id];
}

/** "Liked Songs", a playlist's name, or "Mix: A + B". */
export function sourceName(id: string, sources: ShuffleSource[], t: Translate): string {
  const names = mixMembers(id).map((member) =>
    member === LIKED_SONGS ? t("shuffle.liked") : (sources.find((source) => source.id === member)?.name ?? "…"),
  );
  if (names.length === 1) return names[0] ?? "…";
  return t("shuffle.mix", { names: names.sort((a, b) => a.localeCompare(b)).join(" + ") });
}

export function activityText(activity: ShuffleActivity, t: Translate, language: "de" | "en"): string {
  const count = (value: number) => formatNumber(language, value);
  switch (activity.kind) {
    case "loading":
      return activity.total > 0
        ? t("shuffle.loadingProgress", { loaded: count(activity.loaded), total: count(activity.total) })
        : t("shuffle.loadingTracks");
    case "writing":
      return t("shuffle.writing", { written: count(activity.written), total: count(activity.total) });
    default:
      return t(`shuffle.${activity.kind}` as MessageKey);
  }
}

export function noticeText(notice: ShuffleNotice, t: Translate): string {
  switch (notice.kind) {
    case "kept":
      return t("shuffle.notice.kept", { name: notice.name });
    case "http":
      return t("shuffle.notice.http", { status: notice.status });
    case "failed":
      return t("shuffle.notice.failed", { detail: notice.detail });
    default:
      return t(`shuffle.notice.${notice.kind}` as MessageKey);
  }
}

export function isError(notice: ShuffleNotice): boolean {
  return notice.kind !== "kept";
}

/** The name of a kept shuffle, such as "Liked Songs · Shuffled 15 Nov". */
export function keptName(source: string, language: "de" | "en", t: Translate, date = new Date()): string {
  const day = new Intl.DateTimeFormat(language === "de" ? "de-DE" : "en-GB", { day: "numeric", month: "short" }).format(date);
  return t("shuffle.keptName", { source, date: day });
}

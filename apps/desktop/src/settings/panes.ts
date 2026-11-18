import type { Component } from "svelte";
import type { MessageKey, Translate } from "../lib/i18n";
import type { AppInfo, LyricsView, Playback, Settings } from "../lib/types";
import type { GlyphName } from "../lib/ui/Glyph.svelte";
import About from "./About.svelte";
import Appearance from "./Appearance.svelte";
import General from "./General.svelte";
import SongTiming from "./SongTiming.svelte";

/** What every pane gets: the current state and the changes it can make. */
export interface PaneContext {
  readonly settings: Settings;
  readonly app: AppInfo;
  readonly playback: Playback | null;
  readonly lyrics: LyricsView | null;
  readonly language: "de" | "en";
  readonly t: Translate;
  update(patch: Partial<Settings>): Promise<void>;
  setLaunchAtLogin(enabled: boolean): Promise<void>;
  adjustSongOffset(delta: number): Promise<void>;
  resetSongOffset(): Promise<void>;
}

export interface Pane {
  id: string;
  icon: GlyphName;
  title: MessageKey;
  subtitle: MessageKey;
  component: Component<{ ctx: PaneContext }>;
}

/** The sidebar, top to bottom. A new pane is one more entry. */
export const panes: Pane[] = [
  { id: "general", icon: "general", title: "settings.general", subtitle: "settings.generalSubtitle", component: General },
  { id: "appearance", icon: "appearance", title: "settings.appearance", subtitle: "settings.appearanceSubtitle", component: Appearance },
  { id: "song", icon: "timing", title: "settings.song", subtitle: "settings.songSubtitle", component: SongTiming },
  { id: "about", icon: "about", title: "settings.about", subtitle: "settings.aboutSubtitle", component: About },
];

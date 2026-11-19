// Sing to find as the Rust side reports it (serde camelCase), the commands
// around it and the texts for its problems.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MessageKey, Translate } from "../lib/i18n";
import type { AppInfo } from "../lib/types";

export type SingPhase = "idle" | "preparing" | "listening" | "searching" | "following" | "notFound" | "problem";

export type SingProblem =
  | { kind: "unavailable" }
  | { kind: "noToken" }
  | { kind: "unauthorized" }
  | { kind: "onlineOff" }
  | { kind: "offline" }
  | { kind: "rateLimited" }
  | { kind: "noMicrophone" }
  | { kind: "denied" }
  | { kind: "busy" }
  | { kind: "silent" }
  | { kind: "model"; detail: string }
  | { kind: "failed"; detail: string };

export interface SingView {
  phase: SingPhase;
  /** The words heard so far, while searching or after nothing was found. */
  heard: string;
  song: { title: string; artist: string } | null;
  /** Other candidates "Not this song" can switch to. */
  alternatives: number;
  problem: SingProblem | null;
}

export interface ModelView {
  state: "missing" | "downloading" | "ready" | "failed";
  received: number;
  total: number;
  error: string | null;
  name: string;
  /** Set by SINGLINE_WHISPER_MODEL, so nothing is downloaded. */
  custom: boolean;
}

export interface SingSettings {
  /** Your own Genius token; empty when none was entered. */
  token: string;
  /** This build comes with a token of its own. */
  builtIn: boolean;
  /** This build can recognise singing. */
  available: boolean;
  model: ModelView;
}

export const sing = {
  view: () => invoke<SingView>("sing_view"),
  toggle: () => invoke<SingView>("sing_toggle"),
  stop: () => invoke<void>("sing_stop"),
  next: () => invoke<void>("sing_next"),
  again: () => invoke<void>("sing_again"),
  settings: () => invoke<SingSettings>("sing_settings"),
  setToken: (token: string) => invoke<SingSettings>("sing_set_token", { token }),
  downloadModel: () => invoke<ModelView>("sing_download_model"),
  cancelDownload: () => invoke<ModelView>("sing_cancel_download"),
  removeModel: () => invoke<ModelView>("sing_remove_model"),
  openGenius: () => invoke<void>("sing_open_genius"),
};

export interface SingListeners {
  view?: (view: SingView) => void;
  level?: (level: number) => void;
  model?: (model: ModelView) => void;
}

/** Listens to Sing to find; returns one function to stop. */
export async function onSing(listeners: SingListeners): Promise<UnlistenFn> {
  const stops = await Promise.all([
    listeners.view && listen<SingView>("sing", (event) => listeners.view?.(event.payload)),
    listeners.level && listen<number>("sing-level", (event) => listeners.level?.(event.payload)),
    listeners.model && listen<ModelView>("sing-model", (event) => listeners.model?.(event.payload)),
  ]);
  return () => stops.forEach((stop) => stop?.());
}

/** True while Sing to find listens or follows a song. */
export function isActive(view: SingView | null): boolean {
  return view !== null && ["preparing", "listening", "searching", "following"].includes(view.phase);
}

/** Whole percent of a download, never 100 before it is done. */
export function percent(model: ModelView): number {
  if (model.total <= 0) return 0;
  return Math.min(99, Math.floor((model.received / model.total) * 100));
}

/** Problems the settings can fix, such as a missing token. */
export function needsSettings(problem: SingProblem): boolean {
  return ["noToken", "unauthorized", "onlineOff", "model"].includes(problem.kind);
}

/** What went wrong, in words for the listener's system. */
export function problemText(problem: SingProblem, platform: AppInfo["platform"], t: Translate): string {
  const system = platform === "macos" || platform === "windows" ? platform : "linux";
  switch (problem.kind) {
    case "denied":
    case "silent":
      return t(`sing.problem.${problem.kind}.${system}` as MessageKey);
    case "failed":
      return t("sing.problem.failed", { detail: problem.detail });
    default:
      return t(`sing.problem.${problem.kind}` as MessageKey);
  }
}

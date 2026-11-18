import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ListeningSummary, StatsRange } from "./types";

export const statsApi = {
  summary: (range: StatsRange) => invoke<ListeningSummary>("listening_stats", { range }),
  clear: () => invoke<void>("clear_history"),
};

/** Calls `listener` whenever the listening history changed, and every few seconds while music plays. */
export function onHistory(listener: () => void): Promise<UnlistenFn> {
  return listen("history", () => listener());
}

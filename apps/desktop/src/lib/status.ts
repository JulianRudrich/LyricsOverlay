import type { MessageKey, Translate } from "./i18n";
import type { LyricsView, Playback, Settings } from "./types";

/**
 * What to show instead of lyrics: that nothing plays, what went wrong with the
 * player, or why the song has no lyrics. Empty while lyrics can show.
 */
export function statusText(playback: Playback | null, lyrics: LyricsView | null, settings: Settings | null, t: Translate): string {
  if (!playback || !settings) return "";
  const player = playback.player?.name ?? "";
  if (playback.state === "idle") {
    const source = settings.source;
    return source === "automatic" ? t("status.idleAutomatic") : t("status.idleSource", { player: t(`source.${source}` as MessageKey) });
  }
  if (playback.state === "problem") {
    switch (playback.problem?.kind) {
      case "permissionDenied":
        return t("status.permission", { player });
      case "noTrackData":
        return t("status.noTrackData", { player });
      default:
        return t("status.readFailed", { player });
    }
  }
  switch (lyrics?.status) {
    case "loading":
      return t("lyrics.loading");
    case "instrumental":
      return t("lyrics.instrumental");
    case "notFound":
      return t("lyrics.notFound");
    case "offline":
      return t("lyrics.offline");
    case "rateLimited":
      return t("lyrics.rateLimited");
    case "localOnly":
      return t("lyrics.localOnly");
    case "failed":
      return t("lyrics.failed");
    case "ready":
      return playback.position === null ? t("status.noPosition", { player }) : "";
    default:
      return t("lyrics.loading");
  }
}

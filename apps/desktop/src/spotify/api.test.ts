import { describe, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import { isClientId, isTrackUri, problemText, statusText, type SpotifyStatus } from "./api";

const status = (patch: Partial<SpotifyStatus>): SpotifyStatus => ({
  clientId: "",
  redirectUri: "http://127.0.0.1:43821/callback",
  phase: "disconnected",
  account: null,
  problem: null,
  persistent: true,
  needsReconnect: false,
  loginUrl: null,
  ...patch,
});

describe("spotify", () => {
  it("recognises Client IDs and track URIs", () => {
    expect(isClientId(" 0123456789abcdef0123456789ABCDEF ")).toBe(true);
    expect(isClientId("0123456789abcdef")).toBe(false);
    expect(isClientId("0123456789abcdef0123456789abcdeg")).toBe(false);
    expect(isTrackUri("spotify:track:4uLU6hMCjMI75M1A2tKUQC")).toBe(true);
    expect(isTrackUri("spotify:episode:4uLU6h")).toBe(false);
    expect(isTrackUri("demo")).toBe(false);
    expect(isTrackUri(null)).toBe(false);
  });

  it("describes the connection in both languages", () => {
    const en = translator("en");
    const de = translator("de");
    const connected = status({ phase: "connected", account: { id: "j", name: "Julian" } });
    expect(statusText(connected, en)).toBe("Connected as Julian");
    expect(statusText(connected, de)).toBe("Verbunden als Julian");
    expect(statusText(status({}), en)).toBe("Not connected");
    expect(statusText(status({ problem: { kind: "expired" } }), en)).toContain("expired");
    expect(statusText(status({ phase: "failed", problem: { kind: "offline" } }), de)).toContain("Internetverbindung");
    expect(problemText({ kind: "refused", detail: "Invalid client" }, en)).toBe("Spotify refused the login: Invalid client");
    expect(problemText({ kind: "forbidden", detail: null }, en)).toContain("Premium");
  });

  it("has a text for every problem", () => {
    const en = translator("en");
    const kinds = ["portInUse", "browser", "denied", "timedOut", "offline", "expired"] as const;
    for (const kind of kinds) {
      expect(problemText({ kind }, en)).not.toContain("spotify.problem");
    }
  });
});

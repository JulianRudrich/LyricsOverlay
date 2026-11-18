// The messages between this extension and the Singline app, version 1.
// Shared by the background and the tests; the app checks everything again.
(function (root) {
  "use strict";

  /** The app listens on the first free port of these, on 127.0.0.1 only. */
  const PORTS = [17464, 17465, 17466];
  const LIMITS = { text: 512, url: 2048, tabs: 32, closed: 64 };
  const COMMANDS = ["playPause", "pause", "next", "previous", "seekTo"];

  function text(value, limit = LIMITS.text) {
    return typeof value === "string" ? Array.from(value.trim()).slice(0, limit).join("") : "";
  }

  function number(value, { min = -Infinity, max = Infinity } = {}) {
    return typeof value === "number" && Number.isFinite(value) && value >= min && value <= max ? value : null;
  }

  function address(value) {
    const url = text(value, LIMITS.url);
    return /^https:\/\//i.test(url) ? url : "";
  }

  /** One tab as the app expects it, from whatever the page reported. */
  function sanitizeTab(tabId, state) {
    if (!Number.isInteger(tabId) || tabId < 0 || !state || typeof state !== "object") return null;
    const controls = state.controls && typeof state.controls === "object" ? state.controls : {};
    return {
      tabId,
      url: address(state.url),
      title: text(state.title),
      artist: text(state.artist),
      album: text(state.album),
      artwork: address(state.artwork),
      pageTitle: text(state.pageTitle),
      currentTime: number(state.currentTime, { min: 0 }),
      duration: number(state.duration, { min: 0 }),
      playbackRate: number(state.playbackRate, { min: 0, max: 16 }),
      paused: state.paused !== false,
      ended: state.ended === true,
      sampledAt: number(state.sampledAt, { min: 0 }),
      controls: {
        playPause: controls.playPause === true,
        next: controls.next === true,
        previous: controls.previous === true,
        seek: controls.seek === true,
      },
    };
  }

  /** The body of POST /v1/update. */
  function buildUpdate(tabs, closed) {
    return {
      tabs: tabs.slice(0, LIMITS.tabs),
      closed: closed.filter((tabId) => Number.isInteger(tabId) && tabId >= 0).slice(0, LIMITS.closed),
    };
  }

  /** The commands in the app's answer that this extension can carry out. */
  function parseCommands(answer) {
    const commands = answer && Array.isArray(answer.commands) ? answer.commands : [];
    return commands.filter(
      (entry) =>
        entry &&
        Number.isInteger(entry.tabId) &&
        entry.command &&
        COMMANDS.includes(entry.command.kind) &&
        (entry.command.kind !== "seekTo" || number(entry.command.position, { min: 0 }) !== null),
    );
  }

  /** The six digits of a pairing code, typed with or without spaces, or null. */
  function normalizeCode(input) {
    const digits = typeof input === "string" ? input.replace(/[\s-]/g, "") : "";
    return /^\d{6}$/.test(digits) ? digits : null;
  }

  function endpoint(port, path) {
    return `http://127.0.0.1:${port}${path}`;
  }

  const exported = { PORTS, LIMITS, sanitizeTab, buildUpdate, parseCommands, normalizeCode, endpoint };
  if (typeof module === "object" && module.exports) module.exports = exported;
  else root.SinglineProtocol = exported;
})(globalThis);

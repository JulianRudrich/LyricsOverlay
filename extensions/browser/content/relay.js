// Runs in the extension's isolated world: passes what page.js reports on to
// the background, and commands from the background back to page.js.
(function () {
  "use strict";

  const api = globalThis.browser ?? globalThis.chrome;
  const CHANNEL = "singline-v1";
  let alive = true;

  function send(message) {
    if (!alive) return;
    try {
      const sent = api.runtime.sendMessage(message);
      if (sent && typeof sent.catch === "function") sent.catch(() => {});
    } catch {
      // The extension was reloaded or removed; this page keeps an old copy.
      alive = false;
    }
  }

  window.addEventListener("message", (event) => {
    const data = event.data;
    if (event.source !== window || !data || data.channel !== CHANNEL || data.from !== "page") return;
    if (data.type === "state" && data.state && typeof data.state === "object") {
      send({ type: "state", state: data.state });
    }
  });

  api.runtime.onMessage.addListener((message) => {
    if (message && message.type === "command" && message.command) {
      window.postMessage({ channel: CHANNEL, from: "relay", type: "command", command: message.command }, location.origin);
    }
  });

  window.addEventListener("pagehide", () => send({ type: "gone" }));
})();

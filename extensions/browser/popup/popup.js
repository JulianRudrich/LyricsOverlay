// The popup: shows whether Singline is connected, pairs with the code Singline
// shows, and lists the tabs this browser shares.
(function () {
  "use strict";

  const api = globalThis.browser ?? globalThis.chrome;
  const message = (key, substitutions) => api.i18n.getMessage(key, substitutions) || key;
  const element = (id) => document.getElementById(id);

  for (const node of document.querySelectorAll("[data-message]")) node.textContent = message(node.dataset.message);
  for (const node of document.querySelectorAll("[data-label]")) node.setAttribute("aria-label", message(node.dataset.label));

  const ERRORS = {
    invalidCode: "errorInvalidCode",
    wrongCode: "errorWrongCode",
    pairingClosed: "errorPairingClosed",
    notRunning: "errorNotRunning",
  };

  function site(url) {
    try {
      const host = new URL(url).hostname;
      if (host === "music.youtube.com") return "YouTube Music";
      if (host.endsWith("youtube.com")) return "YouTube";
      if (host === "open.spotify.com") return "Spotify";
      if (host === "music.apple.com") return "Apple Music";
      if (host.startsWith("music.amazon.")) return "Amazon Music";
      return host;
    } catch {
      return "";
    }
  }

  function render(status) {
    const connected = status.paired && (status.state === "connected" || status.state === "idle");
    const [title, hint] = !status.paired
      ? ["popupNotPaired", "popupNotPairedHint"]
      : connected
        ? ["popupConnected", "popupConnectedHint"]
        : status.state === "unreachable"
          ? ["popupUnreachable", "popupUnreachableHint"]
          : ["popupProblem", "popupProblemHint"];
    element("state").textContent = message(title);
    element("state-hint").textContent = message(hint, [String(status.port ?? "")]);
    element("dot").classList.toggle("on", connected);
    element("pairing").hidden = status.paired;
    element("tabs").hidden = !status.paired;
    element("paired-footer").hidden = !status.paired;

    const list = element("tab-list");
    list.replaceChildren(
      ...status.tabs.map((tab) => {
        const item = document.createElement("li");
        const name = document.createElement("span");
        name.className = "title";
        name.textContent = tab.title;
        const details = document.createElement("span");
        details.className = "hint";
        details.textContent = [tab.artist, site(tab.url), tab.paused ? message("popupPaused") : ""]
          .filter(Boolean)
          .join(" · ");
        item.append(name, details);
        return item;
      }),
    );
    element("no-tabs").hidden = status.tabs.length > 0;
  }

  async function refresh() {
    try {
      render(await api.runtime.sendMessage({ type: "status" }));
    } catch {
      render({ paired: false, state: "unknown", port: null, tabs: [] });
    }
  }

  element("pair-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = element("pair-button");
    const error = element("pair-error");
    button.disabled = true;
    error.hidden = true;
    try {
      const result = await api.runtime.sendMessage({ type: "pair", code: element("code").value });
      if (!result || !result.ok) {
        error.textContent = message(ERRORS[result && result.error] || "errorFailed");
        error.hidden = false;
      } else {
        element("code").value = "";
      }
    } finally {
      button.disabled = false;
      await refresh();
    }
  });

  element("unpair").addEventListener("click", async () => {
    await api.runtime.sendMessage({ type: "unpair" });
    await refresh();
  });

  void refresh();
  setInterval(refresh, 2000);
})();

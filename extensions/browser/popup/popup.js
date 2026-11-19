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
  const LENGTH = 6;

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

  // Six boxes, one digit each. Typing moves on, Backspace moves back, and a
  // pasted code fills them all.
  const digits = element("digits");
  const boxes = Array.from({ length: LENGTH }, (_, index) => {
    const box = document.createElement("input");
    box.inputMode = "numeric";
    box.maxLength = 1;
    box.autocomplete = index === 0 ? "one-time-code" : "off";
    box.setAttribute("aria-label", `${message("popupCodeLabel")} ${index + 1}/${LENGTH}`);
    digits.append(box);
    return box;
  });
  const code = () => boxes.map((box) => box.value).join("");

  function fill(text, from) {
    const values = text.replace(/\D/g, "").slice(0, LENGTH - from).split("");
    values.forEach((value, offset) => (boxes[from + offset].value = value));
    const next = Math.min(from + values.length, LENGTH - 1);
    boxes[next].focus();
    changed();
  }

  function changed() {
    const complete = code().length === LENGTH;
    element("pair-button").disabled = !complete;
    if (complete) element("pair-form").requestSubmit();
  }

  boxes.forEach((box, index) => {
    box.addEventListener("input", () => {
      const typed = box.value.replace(/\D/g, "");
      box.value = "";
      if (typed) fill(typed, index);
      else changed();
    });
    box.addEventListener("keydown", (event) => {
      if (event.key === "Backspace" && !box.value && index > 0) {
        boxes[index - 1].value = "";
        boxes[index - 1].focus();
        event.preventDefault();
        changed();
      } else if (event.key === "ArrowLeft" && index > 0) {
        boxes[index - 1].focus();
      } else if (event.key === "ArrowRight" && index < LENGTH - 1) {
        boxes[index + 1].focus();
      }
    });
    box.addEventListener("paste", (event) => {
      event.preventDefault();
      fill(event.clipboardData?.getData("text") ?? "", index);
    });
    box.addEventListener("focus", () => box.select());
  });

  function reject(errorKey) {
    const error = element("pair-error");
    error.textContent = message(errorKey);
    error.hidden = false;
    boxes.forEach((box) => (box.value = ""));
    digits.classList.remove("shake");
    void digits.offsetWidth;
    digits.classList.add("shake");
    boxes[0].focus();
    element("pair-button").disabled = true;
  }

  function tabItem(tab) {
    const item = document.createElement("li");
    item.classList.toggle("paused", Boolean(tab.paused));
    const tile = document.createElement("span");
    tile.className = "tile";
    const bars = document.createElement("span");
    bars.className = "bars";
    bars.append(document.createElement("span"), document.createElement("span"), document.createElement("span"));
    tile.append(bars);
    const text = document.createElement("div");
    text.className = "text";
    const name = document.createElement("span");
    name.className = "title";
    name.textContent = tab.title;
    const details = document.createElement("span");
    details.className = "hint";
    details.textContent = [tab.artist, site(tab.url), tab.paused ? message("popupPaused") : ""].filter(Boolean).join(" · ");
    text.append(name, details);
    item.append(tile, text);
    return item;
  }

  let wasPaired = null;

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
    element("pill").classList.toggle("on", connected);
    element("pill-label").textContent = message(
      !status.paired ? "pillNotPaired" : connected ? "pillConnected" : "pillOffline",
    );

    element("pairing").hidden = status.paired;
    element("tabs").hidden = !status.paired;
    element("paired-footer").hidden = !status.paired;
    if (!status.paired && wasPaired !== false) boxes[0].focus();
    wasPaired = status.paired;

    element("tab-list").replaceChildren(...status.tabs.map(tabItem));
    element("no-tabs").hidden = status.tabs.length > 0;
  }

  async function refresh() {
    try {
      render(await api.runtime.sendMessage({ type: "status" }));
    } catch {
      render({ paired: false, state: "unknown", port: null, tabs: [] });
    }
  }

  let pairing = false;
  element("pair-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    if (pairing || code().length !== LENGTH) return;
    pairing = true;
    const button = element("pair-button");
    button.disabled = true;
    element("pair-error").hidden = true;
    try {
      const result = await api.runtime.sendMessage({ type: "pair", code: code() });
      if (!result || !result.ok) reject(ERRORS[result && result.error] || "errorFailed");
      else boxes.forEach((box) => (box.value = ""));
    } finally {
      pairing = false;
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

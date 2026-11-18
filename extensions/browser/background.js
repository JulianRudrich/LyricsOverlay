// The background of the extension: a service worker in Chrome and Edge, an
// event page in Firefox (which loads lib/ itself, see manifest.json). Wires
// the browser's APIs to the connection in lib/client.js.
if (typeof importScripts === "function") importScripts("lib/protocol.js", "lib/client.js");

const api = globalThis.browser ?? globalThis.chrome;

function browserName() {
  const brands = (navigator.userAgentData && navigator.userAgentData.brands) || [];
  for (const [brand, name] of [
    ["Microsoft Edge", "Edge"],
    ["Opera", "Opera"],
    ["Brave", "Brave"],
    ["Vivaldi", "Vivaldi"],
    ["Google Chrome", "Chrome"],
    ["Chromium", "Chromium"],
  ]) {
    if (brands.some((entry) => entry.brand === brand)) return name;
  }
  const agent = navigator.userAgent || "";
  if (/Firefox\//.test(agent)) return "Firefox";
  if (/Edg\//.test(agent)) return "Edge";
  if (/Chrome\//.test(agent)) return "Chrome";
  return "Browser";
}

const client = globalThis.SinglineClient.createClient({
  fetch: (...args) => fetch(...args),
  storage: {
    get: async () => (await api.storage.local.get("bridge")).bridge ?? null,
    set: (config) => api.storage.local.set({ bridge: config }),
    clear: () => api.storage.local.remove("bridge"),
  },
  now: () => Date.now(),
  deliver: (tabId, command) => {
    const sent = api.tabs.sendMessage(tabId, { type: "command", command });
    if (sent && typeof sent.catch === "function") sent.catch(() => {});
  },
  browser: browserName(),
});

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message || sender.id !== api.runtime.id) return false;
  const tabId = sender.tab && sender.tab.id;
  if (Number.isInteger(tabId)) {
    // From a content script: only what a tab may report about itself.
    if (message.type === "state") void client.updateTab(tabId, message.state);
    else if (message.type === "gone") void client.closeTab(tabId);
    return false;
  }
  // From the popup.
  const answer =
    message.type === "status"
      ? client.status()
      : message.type === "pair"
        ? client.pair(message.code)
        : message.type === "unpair"
          ? client.unpair()
          : null;
  if (!answer) return false;
  answer.then(sendResponse, (error) => sendResponse({ ok: false, error: String(error) }));
  return true;
});

api.tabs.onRemoved.addListener((tabId) => void client.closeTab(tabId));

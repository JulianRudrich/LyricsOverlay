// The connection to the Singline app: pairing, sending what the tabs play,
// and handing the app's commands to the tabs. Browser APIs come in from
// background.js, so the tests can drive it with stand-ins.
(function (root) {
  "use strict";

  const protocol = typeof module === "object" && module.exports ? require("./protocol.js") : root.SinglineProtocol;

  /** After the app was unreachable, wait this long before trying again. */
  const RETRY_MS = 3000;
  /** How long a request may take; the app answers within milliseconds. */
  const TIMEOUT_MS = 4000;

  /**
   * @param {object} options
   * @param {typeof fetch} options.fetch
   * @param {{get(): Promise<object|null>, set(config: object): Promise<void>, clear(): Promise<void>}} options.storage
   * @param {() => number} options.now
   * @param {(tabId: number, command: object) => void} options.deliver
   * @param {string} options.browser The browser's name, shown in Singline.
   * @param {number[]} [options.ports]
   */
  function createClient({ fetch, storage, now, deliver, browser, ports = protocol.PORTS }) {
    let config = null;
    const loading = storage.get().then(
      (stored) => {
        config = valid(stored) ? stored : null;
      },
      () => {},
    );
    const pending = new Map();
    const closed = new Set();
    const shown = new Map();
    let state = "unknown";
    let lastContact = 0;
    let retryAt = 0;
    let sending = false;

    function valid(stored) {
      return (
        stored &&
        Number.isInteger(stored.port) &&
        typeof stored.token === "string" &&
        /^[0-9a-f]{64}$/.test(stored.token) &&
        typeof stored.clientId === "string"
      );
    }

    async function request(port, path, { method = "GET", body, token } = {}) {
      const headers = {};
      if (body !== undefined) headers["Content-Type"] = "application/json";
      if (token) headers.Authorization = `Bearer ${token}`;
      const controller = typeof AbortController === "function" ? new AbortController() : null;
      const timer = controller ? setTimeout(() => controller.abort(), TIMEOUT_MS) : null;
      try {
        const response = await fetch(protocol.endpoint(port, path), {
          method,
          headers,
          body: body === undefined ? undefined : JSON.stringify(body),
          signal: controller ? controller.signal : undefined,
          cache: "no-store",
          credentials: "omit",
        });
        let answer = null;
        try {
          answer = await response.json();
        } catch {
          answer = null;
        }
        return { status: response.status, answer };
      } finally {
        if (timer) clearTimeout(timer);
      }
    }

    /** The port where Singline answers, starting with the one known to work. */
    async function findApp() {
      const order = config ? [config.port, ...ports.filter((port) => port !== config.port)] : ports;
      for (const port of order) {
        try {
          const { status, answer } = await request(port, "/v1/hello");
          if (status === 200 && answer && answer.app === "singline") return port;
        } catch {
          // Nothing listens there.
        }
      }
      return null;
    }

    async function flush() {
      await loading;
      if (sending || !config || (pending.size === 0 && closed.size === 0) || now() < retryAt) return;
      sending = true;
      const tabs = [...pending.values()];
      const gone = [...closed];
      pending.clear();
      closed.clear();
      try {
        const { status, answer } = await request(config.port, "/v1/update", {
          method: "POST",
          body: protocol.buildUpdate(tabs, gone),
          token: config.token,
        });
        if (status === 401) {
          // Singline forgot this browser, for example after "Remove" in its settings.
          state = "notPaired";
          config = null;
          await storage.clear();
        } else if (status === 200) {
          state = "connected";
          lastContact = now();
          for (const { tabId, command } of protocol.parseCommands(answer)) deliver(tabId, command);
        } else {
          state = "error";
        }
      } catch {
        state = "unreachable";
        retryAt = now() + RETRY_MS;
        for (const tab of tabs) if (!pending.has(tab.tabId)) pending.set(tab.tabId, tab);
        for (const tabId of gone) closed.add(tabId);
        const port = await findApp();
        if (port !== null && config && port !== config.port) {
          config = { ...config, port };
          await storage.set(config);
          retryAt = 0;
        }
      } finally {
        sending = false;
      }
      if (config && (pending.size > 0 || closed.size > 0) && now() >= retryAt) await flush();
    }

    return {
      /** A tab reported what it plays. */
      async updateTab(tabId, raw) {
        const tab = protocol.sanitizeTab(tabId, raw);
        if (!tab) return;
        pending.set(tabId, tab);
        closed.delete(tabId);
        shown.set(tabId, { title: tab.title, artist: tab.artist, url: tab.url, paused: tab.paused, at: now() });
        await flush();
      },

      /** A tab was closed or left the site. */
      async closeTab(tabId) {
        const known = shown.delete(tabId) || pending.delete(tabId);
        if (!known) return;
        pending.delete(tabId);
        closed.add(tabId);
        await flush();
      },

      async pair(input) {
        await loading;
        const code = protocol.normalizeCode(input);
        if (!code) return { ok: false, error: "invalidCode" };
        const port = await findApp();
        if (port === null) return { ok: false, error: "notRunning" };
        let reply;
        try {
          reply = await request(port, "/v1/pair", { method: "POST", body: { code, browser } });
        } catch {
          return { ok: false, error: "notRunning" };
        }
        const { status, answer } = reply;
        if (status === 200 && answer && valid({ port, token: answer.token, clientId: answer.clientId })) {
          config = { port, token: answer.token, clientId: answer.clientId };
          await storage.set(config);
          state = "connected";
          lastContact = now();
          retryAt = 0;
          return { ok: true };
        }
        return { ok: false, error: (answer && typeof answer.error === "string" && answer.error) || "failed" };
      },

      async unpair() {
        await loading;
        if (config) {
          try {
            await request(config.port, "/v1/unpair", { method: "POST", body: {}, token: config.token });
          } catch {
            // Singline is closed; it forgets the browser when you remove it there.
          }
        }
        config = null;
        state = "notPaired";
        await storage.clear();
        return { ok: true };
      },

      /** What the popup shows. */
      async status() {
        await loading;
        if (!config) state = "notPaired";
        else if (state === "unknown" || state === "unreachable") {
          const port = await findApp();
          state = port === null ? "unreachable" : state === "unknown" ? "idle" : "connected";
        }
        const recent = [...shown.entries()]
          .filter(([, tab]) => now() - tab.at < 120_000 && tab.title)
          .map(([tabId, tab]) => ({ tabId, ...tab }));
        return { paired: config !== null, state, port: config ? config.port : null, lastContact, tabs: recent };
      },
    };
  }

  const exported = { createClient, RETRY_MS };
  if (typeof module === "object" && module.exports) module.exports = exported;
  else root.SinglineClient = exported;
})(globalThis);

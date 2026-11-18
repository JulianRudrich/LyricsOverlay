"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { createClient, RETRY_MS } = require("../lib/client.js");

const TOKEN = "ab".repeat(32);

/** A pretend Singline behind a pretend fetch, listening on one port. */
function pretendApp({ port = 17465, code = "482913" } = {}) {
  const app = { port, code, running: true, updates: [], commands: [], unpaired: 0, token: TOKEN };
  app.fetch = async (url, options = {}) => {
    const { port: target, pathname } = new URL(url);
    if (!app.running || Number(target) !== app.port) throw new TypeError("Failed to fetch");
    const reply = (status, body) => ({ status, json: async () => body });
    const authorized = options.headers && options.headers.Authorization === `Bearer ${app.token}`;
    const body = options.body ? JSON.parse(options.body) : null;
    switch (pathname) {
      case "/v1/hello":
        return reply(200, { app: "singline", protocol: 1 });
      case "/v1/pair":
        return body.code === app.code
          ? reply(200, { token: app.token, clientId: "c1" })
          : reply(403, { error: "wrongCode" });
      case "/v1/update":
        if (!authorized) return reply(401, { error: "unauthorized" });
        app.updates.push(body);
        return reply(200, { commands: app.commands.splice(0) });
      case "/v1/unpair":
        app.unpaired += 1;
        return reply(authorized ? 200 : 401, {});
      default:
        return reply(404, { error: "notFound" });
    }
  };
  return app;
}

function memoryStorage(initial = null) {
  const storage = {
    value: initial,
    get: async () => storage.value,
    set: async (value) => {
      storage.value = value;
    },
    clear: async () => {
      storage.value = null;
    },
  };
  return storage;
}

function setup(app, storage = memoryStorage()) {
  const clock = { now: 1_000_000 };
  const delivered = [];
  const client = createClient({
    fetch: app.fetch,
    storage,
    now: () => clock.now,
    deliver: (tabId, command) => delivered.push([tabId, command.kind]),
    browser: "Firefox",
  });
  return { client, storage, clock, delivered };
}

const tab = (title, extra = {}) => ({ url: "https://music.youtube.com/watch?v=1", title, artist: "Artist", ...extra });

test("pairs on the port where Singline answers", async () => {
  const app = pretendApp();
  const { client, storage } = setup(app);
  assert.deepEqual(await client.pair("123456"), { ok: false, error: "wrongCode" });
  assert.equal(storage.value, null);
  assert.deepEqual(await client.pair("482 913"), { ok: true });
  assert.deepEqual(storage.value, { port: 17465, token: TOKEN, clientId: "c1" });
  assert.equal((await client.status()).paired, true);
});

test("sends what a tab plays and hands commands to it", async () => {
  const app = pretendApp();
  const { client, delivered } = setup(app, memoryStorage({ port: 17465, token: TOKEN, clientId: "c1" }));
  app.commands.push({ tabId: 5, command: { kind: "playPause" } });
  await client.updateTab(5, tab("Song", { currentTime: 3, duration: 200, paused: false }));
  assert.equal(app.updates.length, 1);
  assert.equal(app.updates[0].tabs[0].title, "Song");
  assert.equal(app.updates[0].tabs[0].tabId, 5);
  assert.deepEqual(delivered, [[5, "playPause"]]);

  await client.closeTab(5);
  assert.deepEqual(app.updates[1], { tabs: [], closed: [5] });
  await client.closeTab(5);
  assert.equal(app.updates.length, 2, "a tab is closed once");

  const status = await client.status();
  assert.equal(status.state, "connected");
  assert.deepEqual(status.tabs, []);
});

test("does nothing before pairing", async () => {
  const app = pretendApp();
  const { client } = setup(app);
  await client.updateTab(1, tab("Song"));
  assert.equal(app.updates.length, 0);
  assert.equal((await client.status()).state, "notPaired");
  assert.deepEqual(await client.pair("1234"), { ok: false, error: "invalidCode" });
});

test("waits while Singline is closed and finds it again", async () => {
  const app = pretendApp({ port: 17464 });
  const { client, clock, storage } = setup(app, memoryStorage({ port: 17464, token: TOKEN, clientId: "c1" }));
  app.running = false;
  await client.updateTab(1, tab("First"));
  assert.equal((await client.status()).state, "unreachable");
  app.running = true;
  app.port = 17466;
  await client.updateTab(1, tab("Second"));
  assert.equal(app.updates.length, 0, "no new attempt before the retry time");
  clock.now += RETRY_MS;
  await client.updateTab(1, tab("Third"));
  assert.equal(app.updates.length, 1);
  assert.equal(app.updates[0].tabs[0].title, "Third", "only the latest state of a tab is sent");
  assert.equal(storage.value.port, 17466, "the new port is remembered");
});

test("forgets the pairing when Singline no longer knows this browser", async () => {
  const app = pretendApp();
  const { client, storage } = setup(app, memoryStorage({ port: 17465, token: "cd".repeat(32), clientId: "old" }));
  await client.updateTab(1, tab("Song"));
  assert.equal(storage.value, null);
  assert.equal((await client.status()).paired, false);
});

test("unpairing tells Singline and forgets the token", async () => {
  const app = pretendApp();
  const { client, storage } = setup(app, memoryStorage({ port: 17465, token: TOKEN, clientId: "c1" }));
  assert.deepEqual(await client.unpair(), { ok: true });
  assert.equal(app.unpaired, 1);
  assert.equal(storage.value, null);
});

test("ignores a stored pairing that does not look right", async () => {
  const app = pretendApp();
  const { client } = setup(app, memoryStorage({ port: "17465", token: "short", clientId: 1 }));
  assert.equal((await client.status()).paired, false);
});

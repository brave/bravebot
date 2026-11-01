// The service worker against fake extension events and native messaging ports.

import assert from "node:assert/strict";
import { mock, test } from "node:test";
import { ACTIVITY_KEY } from "../activity.js";
import { AWAKE_KEY } from "../awake.js";
import { SCOPE_ALL } from "../groups.js";
import { HOUR_MS, LIFETIMES_KEY, RUNS_KEY } from "../lifetimes.js";
import { GROUP_SWITCHES_KEY } from "../switches.js";
import { requestsOf } from "../requests.js";
import { ALLOWED_SITES_KEY, SETTINGS_KEY, SITE_SCOPES_KEY } from "../tools.js";
import { fakeToolbar } from "./fake-chrome.mjs";
import { withMenus } from "./fake-menus.mjs";
import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

function event() {
  const listeners = [];
  return {
    addListener(listener) {
      listeners.push(listener);
    },
    fire(...arguments_) {
      for (const listener of listeners) {
        listener(...arguments_);
      }
    },
  };
}

// What the worker needs of `chrome.runtime` to set up the confirmation window,
// which these tests leave alone: its address, the connections to it and the
// windows an earlier worker left open.
const CONFIRMATION_RUNTIME = {
  id: "fake-extension-id",
  getURL: (path) => `chrome-extension://fake-extension-id/${path}`,
  onConnect: { addListener() {} },
  onMessage: { addListener() {} },
  async getContexts() {
    return [];
  },
};

// A port that records what is posted to it, and refuses once disconnected as
// a real one does.
function port() {
  const made = {
    sent: [],
    attempts: 0,
    disconnected: false,
    onMessage: event(),
    onDisconnect: event(),
    postMessage(message) {
      made.attempts += 1;
      if (made.disconnected) {
        throw new Error("Attempting to use a disconnected port object");
      }
      made.sent.push(message);
    },
  };
  made.onDisconnect.addListener(() => {
    made.disconnected = true;
  });
  return made;
}

// The network-watch group on all sites, which needs both host patterns held.
// Limited to the allowed sites, with none allowed, it needs no host.
const EVERY_SITE = { [SITE_SCOPES_KEY]: { "network-watch": SCOPE_ALL } };

// Storage that keeps what is saved in `stored`, and session storage that keeps
// what is saved in `session`. A save is reported to `onChanged` once it is
// stored, with the area it was saved in.
function storage(stored = {}, session = {}) {
  const onChanged = event();
  return {
    local: fakeArea(stored, (changes) => onChanged.fire(changes, "local")),
    session: fakeArea(session, (changes) => onChanged.fire(changes, "session")),
    onChanged,
  };
}

// Lets queued promise callbacks run, a bounded number of times, until
// `condition` holds.
async function settle(condition) {
  for (let turn = 0; turn < 100 && !condition(); turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  return condition();
}

test("the worker connects at each start and after a disconnect", async () => {
  const connected = [];
  const madeAlarms = [];
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    ...CONFIRMATION_RUNTIME,
    onInstalled: event(),
    connectNative(host) {
      const connectedPort = port();
      connected.push({ host, port: connectedPort });
      return connectedPort;
    },
  };
  const alarms = {
    onAlarm: event(),
    create(name, options) {
      madeAlarms.push({ name, options });
    },
  };
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime,
    alarms,
    permissions: fakePermissions(),
    storage: storage(),
  };
  console.info = () => {};
  try {
    await import(`../background.js?test=${Date.now()}`);
    assert.equal(connected.length, 1);
    assert.equal(connected[0].host, "com.brave.bravebot");
    assert.deepEqual(madeAlarms, [
      { name: "reconnect", options: { periodInMinutes: 1 } },
    ]);

    runtime.onStartup.fire();
    assert.equal(connected.length, 1, "an open port was replaced");
    connected[0].port.onDisconnect.fire();
    runtime.onStartup.fire();
    assert.equal(connected.length, 2);

    connected[1].port.onDisconnect.fire();
    alarms.onAlarm.fire({ name: "reconnect" });
    assert.equal(connected.length, 3);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// The tools of the input group do nothing until a window allows them, so the
// worker has to set the window up as it starts: listen for the window's port and
// its closing, give input.js the window to ask, and close the windows a worker
// that stopped left open.
test("the worker sets up the confirmation window as it starts, and the input tools ask it", async () => {
  const created = [];
  const removed = [];
  const connections = [];
  const messages = [];
  const closing = [];
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    onInstalled: event(),
    ...CONFIRMATION_RUNTIME,
    onConnect: { addListener: (listener) => connections.push(listener) },
    onMessage: { addListener: (listener) => messages.push(listener) },
    async getContexts() {
      return [
        { contextType: "TAB", documentUrl: "chrome-extension://fake-extension-id/confirm.html", windowId: 77 },
        { contextType: "TAB", documentUrl: "chrome-extension://fake-extension-id/options.html", windowId: 78 },
      ];
    },
    connectNative: () => port(),
  };
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime,
    alarms: { onAlarm: event(), create() {} },
    permissions: fakePermissions(),
    storage: storage(),
    windows: {
      onRemoved: { addListener: (listener) => closing.push(listener) },
      create: (options) => {
        created.push(options);
        return new Promise(() => {});
      },
      remove: async (id) => {
        removed.push(id);
      },
    },
  };
  console.info = () => {};
  // The question the test asks starts the time limit, which the test does not wait out.
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    await import(`../background.js?test=confirmation-${Date.now()}`);
    assert.equal(connections.length, 1, "the worker does not listen for the window's port");
    // One listener of the confirmation window and one of the capture window.
    assert.equal(closing.length, 2, "the worker does not listen for the windows closing");
    assert.equal(messages.length, 1, "the worker does not listen for the capture window's message");
    await settle(() => removed.length > 0);
    assert.deepEqual(removed, [77], "only the window of the confirmation page is closed");
    // input.js asks the window the worker set: a question opens a popup window.
    const { confirm } = await import("../approval.js");
    confirm({ description: {} });
    await settle(() => created.length > 0);
    assert.equal(created.length, 1);
    assert.equal(created[0].url, "chrome-extension://fake-extension-id/confirm.html");
    assert.equal(created[0].type, "popup");
  } finally {
    mock.timers.reset();
    const { setConfirmation } = await import("../approval.js");
    setConfirmation();
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A request answered after its port closed and another opened goes back on
// its own port, which refuses it, and never to the new port's host. That host
// numbers its requests from 1 as well, so a reply reaching it could be taken
// for the answer to a request of its own.
test("a reply goes back on the port its request came in on", async () => {
  const connected = [];
  let release;
  const read = new Promise((resolve) => {
    release = resolve;
  });
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    ...CONFIRMATION_RUNTIME,
    onInstalled: event(),
    connectNative() {
      const made = port();
      connected.push(made);
      return made;
    },
  };
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime,
    alarms: { onAlarm: event(), create() {} },
    permissions: fakePermissions(),
    storage: { local: { get: () => read }, onChanged: event() },
    tabs: { query: async () => [] },
  };
  console.info = () => {};
  try {
    await import(`../background.js?reply=${Date.now()}`);
    const [first] = connected;
    first.onMessage.fire({ id: 5, method: "list_tabs", params: {} });
    first.onDisconnect.fire();
    runtime.onStartup.fire();
    const second = connected[1];
    assert.ok(second, "the worker did not reconnect");

    release({});
    assert.ok(await settle(() => first.attempts === 1));
    assert.deepEqual(second.sent, [], "a reply reached the new port's host");

    second.onMessage.fire({ id: 1, method: "list_tabs", params: {} });
    assert.ok(await settle(() => second.sent.length === 1));
    assert.equal(second.sent[0].id, 1);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A worker with the groups tabs-read and history-read turned on and their
// permissions granted, as the options page leaves them.
async function workerWithGroupsOn(tag) {
  const stored = {
    [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
    [SETTINGS_KEY]: { list_tabs: true, list_frames: true, search_history: true },
  };
  const calls = [];
  const permissions = fakePermissions({ held: allOptional(), calls });
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    ...CONFIRMATION_RUNTIME,
    onInstalled: event(),
    connectNative: () => port(),
  };
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime,
    alarms: { onAlarm: event(), create() {} },
    permissions,
    storage: storage(stored),
  };
  await import(`../background.js?${tag}=${Date.now()}`);
  return { stored, calls, permissions, runtime };
}

// The browser tells the worker when a permission is taken back in its
// extension settings. The group that needed it is turned off at once, with its
// tools, so that granting the permission again for another group does not
// bring them back. The options page may not be open to do it.
test("a permission taken back turns off the groups that needed it, and their tools", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { stored, calls, permissions } = await workerWithGroupsOn("revoked");
    await permissions.revoke({ permissions: ["history"] });
    assert.ok(
      await settle(() => stored[GROUP_SWITCHES_KEY]["history-read"] !== true),
      "the group was not turned off",
    );
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "tabs-read": true });
    assert.equal(stored[SETTINGS_KEY].search_history, false);
    assert.equal(stored[SETTINGS_KEY].list_tabs, true);
    assert.equal(stored[SETTINGS_KEY].list_frames, true);
    assert.deepEqual(
      calls.filter(([name]) => name === "permissions.remove"),
      [["permissions.remove", { permissions: ["history", "topSites"] }]],
      "what the other group still needs was taken back, or nothing was",
    );
    assert.equal(permissions.held.permissions.has("tabs"), true);
    assert.equal(permissions.held.permissions.has("sessions"), true);

    await permissions.request({ permissions: ["history"] });
    await settle(() => false);
    assert.equal(stored[SETTINGS_KEY].search_history, false);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A permission can go while Brave is closed, when no event is delivered.
test("each start turns off the groups whose permissions are no longer held", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { stored, permissions, runtime } = await workerWithGroupsOn("start");
    permissions.held.permissions.delete("tabs");
    runtime.onStartup.fire();
    assert.ok(
      await settle(() => stored[GROUP_SWITCHES_KEY]["tabs-read"] !== true),
      "the group was not turned off",
    );
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
    assert.equal(stored[SETTINGS_KEY].list_tabs, false);
    assert.equal(stored[SETTINGS_KEY].search_history, true);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A check the browser does not answer says nothing about what is held, so the
// groups stay as they are, and the worker does not fail on it.
test("a permission check that fails leaves the groups on and is reported", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const { stored, permissions, runtime } = await workerWithGroupsOn("failed");
    permissions.contains = async () => {
      throw new Error("the browser did not answer");
    };
    runtime.onStartup.fire();
    assert.ok(
      await settle(() => said.some((line) => line.includes("not checked"))),
      "the failure was not reported",
    );
    assert.match(said.join("\n"), /the browser did not answer/);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], {
      "tabs-read": true,
      "history-read": true,
    });
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A worker with the groups in `stored` turned on, as the options page leaves
// them, and `session` as Brave holds it, which is empty after a restart. It
// returns the alarm event so a test can fire it.
async function workerWithLifetimes(tag, { stored, session }) {
  const calls = [];
  const permissions = fakePermissions({ held: allOptional(), calls });
  const alarms = { onAlarm: event(), create() {} };
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime: {
      lastError: undefined,
      onStartup: event(),
    ...CONFIRMATION_RUNTIME,
      onInstalled: event(),
      connectNative: () => port(),
    },
    alarms,
    permissions,
    storage: storage(stored, session),
  };
  await import(`../background.js?${tag}=${Date.now()}`);
  return { calls, permissions, alarms };
}

const removed = (calls) =>
  calls.filter(([name]) => name === "permissions.remove").map(([, query]) => query);

// Brave restarting empties session storage, and the worker starts with the
// browser, so the first thing it sees is a group turned on until Brave restarts
// with no marker. Another group, turned on until it is turned off, stays.
test("each start turns off a group turned on until Brave restarts once Brave has restarted", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const stored = {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
      [SETTINGS_KEY]: { list_tabs: true, search_history: true },
      [LIFETIMES_KEY]: {
        "tabs-read": { lifetime: "until-restart" },
        "history-read": { lifetime: "until-turned-off" },
      },
    };
    const { calls } = await workerWithLifetimes("restarted", { stored, session: {} });
    assert.ok(
      await settle(() => stored[GROUP_SWITCHES_KEY]["tabs-read"] !== true),
      "the group was not turned off",
    );
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
    assert.equal(stored[SETTINGS_KEY].list_tabs, false);
    assert.equal(stored[SETTINGS_KEY].search_history, true);
    assert.deepEqual(Object.keys(stored[LIFETIMES_KEY]), ["history-read"]);
    // sessions is listed by history-read as well and stays.
    assert.deepEqual(removed(calls), [
      { permissions: ["tabs", "tabGroups", "webNavigation"] },
    ]);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a group turned on until Brave restarts stays on while the worker restarts within the same run", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const stored = {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true },
      [LIFETIMES_KEY]: { "tabs-read": { lifetime: "until-restart" } },
    };
    const { calls } = await workerWithLifetimes("same-run", {
      stored,
      session: { [RUNS_KEY]: { "tabs-read": true } },
    });
    await settle(() => false);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "tabs-read": true });
    assert.deepEqual(removed(calls), []);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The hour is enforced at each call. The alarm that reconnects the port also
// makes the switch and the permissions follow, with no page open.
test("the alarm turns off a group whose hour has passed, and leaves one whose hour has not", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const stored = {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
      [SETTINGS_KEY]: { list_tabs: true, search_history: true },
      [LIFETIMES_KEY]: {
        "tabs-read": { lifetime: "one-hour", expires: Date.now() + HOUR_MS / 2 },
        "history-read": { lifetime: "one-hour", expires: Date.now() + HOUR_MS },
      },
    };
    const { alarms } = await workerWithLifetimes("alarm", { stored, session: {} });
    await settle(() => false);
    assert.deepEqual(Object.keys(stored[GROUP_SWITCHES_KEY]), ["tabs-read", "history-read"]);

    stored[LIFETIMES_KEY]["tabs-read"].expires = Date.now() - 1;
    alarms.onAlarm.fire({ name: "reconnect" });
    assert.ok(
      await settle(() => stored[GROUP_SWITCHES_KEY]["tabs-read"] !== true),
      "the group was not turned off",
    );
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
    assert.equal(stored[SETTINGS_KEY].list_tabs, false);
    assert.equal(stored[SETTINGS_KEY].search_history, true);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a lifetime check that fails is reported and leaves the groups on", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const stored = { [GROUP_SWITCHES_KEY]: { "tabs-read": true } };
    const { alarms } = await workerWithLifetimes("unchecked", { stored, session: {} });
    globalThis.chrome.storage.local.get = async () => {
      throw new Error("storage did not answer");
    };
    alarms.onAlarm.fire({ name: "reconnect" });
    assert.ok(
      await settle(() => said.some((line) => line.includes("lifetimes not checked"))),
      "the failure was not reported",
    );
    assert.match(said.join("\n"), /storage did not answer/);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "tabs-read": true });
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A worker in a browser whose session storage holds `session`, and which has a
// power API and the alarms it clears. `calls` records the power and alarm calls.
async function workerWithPower(tag, { session }) {
  const calls = [];
  const alarms = {
    onAlarm: event(),
    create() {},
    async clear(name) {
      calls.push(["alarms.clear", name]);
    },
  };
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime: {
      lastError: undefined,
      onStartup: event(),
    ...CONFIRMATION_RUNTIME,
      onInstalled: event(),
      connectNative: () => port(),
    },
    alarms,
    power: {
      releaseKeepAwake() {
        calls.push(["power.releaseKeepAwake"]);
      },
    },
    permissions: fakePermissions({ held: allOptional(), calls: [] }),
    storage: storage({}, session),
  };
  await import(`../background.js?${tag}=${Date.now()}`);
  return { calls, alarms };
}

// A request to keep the computer awake ends with its time even if no alarm
// fires for it, so the worker checks as it starts and on every alarm.
test("the worker releases a keep-awake request whose time has passed as it starts, and leaves one that has not", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const lapsed = { [AWAKE_KEY]: { level: "system", until: Date.now() - 1 } };
    const first = await workerWithPower("awake-lapsed", { session: lapsed });
    assert.ok(
      await settle(() => !(AWAKE_KEY in lapsed)),
      "the request was not released",
    );
    assert.deepEqual(first.calls, [
      ["power.releaseKeepAwake"],
      ["alarms.clear", "keep-awake"],
    ]);

    const standing = {
      [AWAKE_KEY]: { level: "display", until: Date.now() + HOUR_MS },
    };
    const second = await workerWithPower("awake-standing", { session: standing });
    await settle(() => false);
    assert.deepEqual(second.calls, []);
    assert.ok(AWAKE_KEY in standing);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("an alarm releases a keep-awake request once its time has passed", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const session = {
      [AWAKE_KEY]: { level: "system", until: Date.now() + HOUR_MS },
    };
    const { calls, alarms } = await workerWithPower("awake-alarm", { session });
    alarms.onAlarm.fire({ name: "keep-awake" });
    await settle(() => false);
    assert.deepEqual(calls, []);

    session[AWAKE_KEY].until = Date.now() - 1;
    alarms.onAlarm.fire({ name: "keep-awake" });
    assert.ok(
      await settle(() => !(AWAKE_KEY in session)),
      "the request was not released",
    );
    assert.deepEqual(calls, [
      ["power.releaseKeepAwake"],
      ["alarms.clear", "keep-awake"],
    ]);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A worker whose browser-info group is on with a keep_awake request standing.
// `power` exists only while the browser holds the permission, as the API does.
// `calls` records the permission, alarm and power calls in order.
async function workerWithAwakeGroup(tag, { lifetime }) {
  const calls = [];
  const permissions = fakePermissions({ held: allOptional(), calls });
  const power = {
    releaseKeepAwake() {
      calls.push(["power.releaseKeepAwake"]);
    },
  };
  const stored = {
    [GROUP_SWITCHES_KEY]: { "browser-info": true },
    [SETTINGS_KEY]: { keep_awake: true },
    [LIFETIMES_KEY]: { "browser-info": lifetime },
  };
  const session = {
    [AWAKE_KEY]: { level: "system", until: Date.now() + HOUR_MS },
  };
  const alarms = {
    onAlarm: event(),
    create() {},
    async clear(name) {
      calls.push(["alarms.clear", name]);
    },
  };
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime: {
      lastError: undefined,
      onStartup: event(),
    ...CONFIRMATION_RUNTIME,
      onInstalled: event(),
      connectNative: () => port(),
    },
    alarms,
    get power() {
      return permissions.held.permissions.has("power") ? power : undefined;
    },
    permissions,
    storage: storage(stored, session),
  };
  await import(`../background.js?${tag}=${Date.now()}`);
  return { calls, alarms, stored, session, permissions };
}

// The names of the calls that release the request or take permissions back.
const awakeOrder = (calls) =>
  calls
    .map(([name]) => name)
    .filter((name) =>
      ["power.releaseKeepAwake", "alarms.clear", "permissions.remove"].includes(name),
    );

// A request made in the last minutes of an hour does not outlive the hour: the
// worker turns the group off at its start and on the alarm, and the request is
// released first, while the browser still holds the power permission.
test("the worker releases the keep-awake request of a browser-info group whose hour has passed, at its start and on the alarm", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const ended = await workerWithAwakeGroup("awake-hour-ended", {
      lifetime: { lifetime: "one-hour", expires: Date.now() - 1 },
    });
    assert.ok(
      await settle(() => awakeOrder(ended.calls).length >= 3),
      "the request was not released",
    );
    assert.deepEqual(ended.stored[GROUP_SWITCHES_KEY], {});
    assert.deepEqual(awakeOrder(ended.calls), [
      "power.releaseKeepAwake",
      "alarms.clear",
      "permissions.remove",
    ]);

    const running = await workerWithAwakeGroup("awake-hour-running", {
      lifetime: { lifetime: "one-hour", expires: Date.now() + HOUR_MS },
    });
    await settle(() => false);
    assert.deepEqual(awakeOrder(running.calls), []);
    running.stored[LIFETIMES_KEY]["browser-info"].expires = Date.now() - 1;
    running.alarms.onAlarm.fire({ name: "reconnect" });
    assert.ok(
      await settle(() => awakeOrder(running.calls).length >= 3),
      "the request was not released",
    );
    assert.deepEqual(awakeOrder(running.calls), [
      "power.releaseKeepAwake",
      "alarms.clear",
      "permissions.remove",
    ]);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a permission of browser-info taken back in the browser makes the worker release the keep-awake request", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { calls, stored, session, permissions } = await workerWithAwakeGroup(
      "awake-revoked",
      { lifetime: { lifetime: "until-turned-off" } },
    );
    await permissions.revoke({ permissions: ["idle"] });
    assert.ok(
      await settle(() => awakeOrder(calls).length >= 3),
      "the request was not released",
    );
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
    assert.deepEqual(awakeOrder(calls), [
      "power.releaseKeepAwake",
      "alarms.clear",
      "permissions.remove",
    ]);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a keep-awake check that fails is reported and leaves the request", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const session = {
      [AWAKE_KEY]: { level: "system", until: Date.now() - 1 },
    };
    const { alarms } = await workerWithPower("awake-failed", { session });
    await settle(() => !(AWAKE_KEY in session));
    session[AWAKE_KEY] = { level: "system", until: Date.now() - 1 };
    globalThis.chrome.storage.session.get = async () => {
      throw new Error("session storage did not answer");
    };
    alarms.onAlarm.fire({ name: "reconnect" });
    assert.ok(
      await settle(() => said.some((line) => line.includes("keep awake not checked"))),
      "the failure was not reported",
    );
    assert.match(said.join("\n"), /session storage did not answer/);
    assert.ok(AWAKE_KEY in session);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// An event whose listeners a test can see, remove and fire.
function seen() {
  const registered = [];
  return {
    registered,
    addListener(listener, filter) {
      registered.push({ listener, filter });
    },
    removeListener(listener) {
      const at = registered.findIndex((entry) => entry.listener === listener);
      if (at >= 0) {
        registered.splice(at, 1);
      }
    },
    fire(details) {
      for (const { listener } of [...registered]) {
        listener(details);
      }
    },
  };
}

// Lets the reads the worker started finish.
async function flush() {
  await new Promise((resolve) => setImmediate(resolve));
}

// The switches that keep requests: the network-watch group and list_requests.
const REQUESTS_ON = {
  [GROUP_SWITCHES_KEY]: { "network-watch": true },
  [SETTINGS_KEY]: { list_requests: true },
};

// A worker started in a browser that holds `held`, with the webRequest events
// it has only while it holds that permission. `stored` is what is saved on top
// of the switches that keep requests, which are on. A `read` that is given is
// called with the key of each read of storage after the value is read, and what
// the read answers is what it found then, whenever `read` lets it.
async function workerWatching(
  tag,
  { held = allOptional(), webRequest = true, stored = {}, read } = {},
) {
  stored = { ...REQUESTS_ON, ...stored };
  const events = {
    onBeforeRequest: seen(),
    onBeforeRedirect: seen(),
    onCompleted: seen(),
    onErrorOccurred: seen(),
  };
  const tabs = { onRemoved: seen() };
  const permissions = fakePermissions({ held });
  globalThis.chrome = {
    ...fakeToolbar().api,
    runtime: {
      lastError: undefined,
      onStartup: event(),
    ...CONFIRMATION_RUNTIME,
      onInstalled: event(),
      connectNative: () => port(),
    },
    alarms: { onAlarm: event(), create() {} },
    permissions,
    storage: storage(stored),
    tabs,
    ...(webRequest ? { webRequest: events } : {}),
  };
  if (read) {
    const get = globalThis.chrome.storage.local.get;
    globalThis.chrome.storage.local.get = async (key) => {
      const value = await get(key);
      await read(key);
      return value;
    };
  }
  await import(`../background.js?${tag}=${Date.now()}`);
  await flush();
  const listening = () =>
    [...Object.values(events), tabs.onRemoved].map(
      (each) => each.registered.length,
    );
  const load = (tabId) =>
    events.onBeforeRequest.fire({
      tabId,
      requestId: `load-${tabId}`,
      type: "main_frame",
      frameId: 0,
      parentFrameId: -1,
      url: `https://page${tabId}.example/`,
    });
  return {
    chrome: globalThis.chrome,
    stored,
    permissions,
    events,
    listening,
    load,
  };
}

// The listeners are in place as the worker starts, since the browser reports
// an event only to a listener registered then.
test("the worker keeps the requests of a tab from its start when the browser holds webRequest", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { chrome, listening, load } = await workerWatching("keeps");
    assert.deepEqual(listening(), [1, 1, 1, 1, 1]);
    load(5);
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A tool that is off touches nothing. The listeners are registered whatever the
// switches are, since the browser reports an event only to a listener registered
// as the worker starts, and a request is kept only while the group's switch and
// the switch of list_requests are both on.
test("a request is kept only while the network-watch group and list_requests are both on", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const cases = [
      [true, true, true],
      [true, false, false],
      [false, true, false],
      [false, false, false],
    ];
    for (const [group, tool, kept] of cases) {
      const { chrome, listening, load } = await workerWatching(
        `switches-${group}-${tool}`,
        {
          stored: {
            [GROUP_SWITCHES_KEY]: group ? { "network-watch": true } : {},
            [SETTINGS_KEY]: { list_requests: tool },
          },
        },
      );
      assert.deepEqual(listening(), [1, 1, 1, 1, 1], "registered as it starts");
      load(5);
      assert.equal(
        requestsOf(chrome, 5) !== undefined,
        kept,
        `group ${group}, tool ${tool}`,
      );
    }
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The worker keeps a copy of the two switches and follows the saves. What was
// kept for a switch that goes off is forgotten with it.
test("turning the group or list_requests off forgets the requests and keeps none, and turning it on keeps requests from the next load", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { chrome, load } = await workerWatching("follows-switches");
    const save = (items) => chrome.storage.local.set(items);
    load(5);
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);

    await save({ [SETTINGS_KEY]: { list_requests: false } });
    await flush();
    assert.equal(requestsOf(chrome, 5), undefined, "kept after the tool went off");
    load(6);
    assert.equal(requestsOf(chrome, 6), undefined);

    await save({ [SETTINGS_KEY]: { list_requests: true } });
    await flush();
    assert.equal(requestsOf(chrome, 5), undefined, "a page from before");
    load(7);
    assert.equal(requestsOf(chrome, 7)?.requests.length, 1);

    await save({ [GROUP_SWITCHES_KEY]: {} });
    await flush();
    assert.equal(requestsOf(chrome, 7), undefined, "kept after the group went off");
    load(8);
    assert.equal(requestsOf(chrome, 8), undefined);

    await save({ [GROUP_SWITCHES_KEY]: { "network-watch": true } });
    await flush();
    load(9);
    assert.equal(requestsOf(chrome, 9)?.requests.length, 1);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// Saves of other keys, and of the other storage area, change nothing.
test("a save that is not of the two switches, or not in local storage, does not change what is kept", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { chrome, stored, load } = await workerWatching("other-saves");
    load(5);
    stored[SETTINGS_KEY] = { list_requests: false };
    await chrome.storage.local.set({ allowed_sites: [] });
    await chrome.storage.session.set({ [SETTINGS_KEY]: { list_requests: false } });
    await flush();
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);
    load(6);
    assert.equal(requestsOf(chrome, 6)?.requests.length, 1);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// Between the worker starting and the first read of the switches the worker
// does not know that requests are wanted, so a request seen then is not kept.
test("a request seen before the worker has read the switches is not kept", async () => {
  const previousChrome = globalThis.chrome;
  try {
    let release;
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    const { chrome, load } = await workerWatching("before-the-read", {
      read: () => gate,
    });
    load(5);
    assert.equal(requestsOf(chrome, 5), undefined);
    release();
    await flush();
    load(6);
    assert.equal(requestsOf(chrome, 6)?.requests.length, 1);
    assert.equal(requestsOf(chrome, 5), undefined, "the page seen before the read");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a read of the switches that fails keeps no request and is reported, and the next save is read again", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    let failing = true;
    const { chrome, load } = await workerWatching("read-fails", {
      read: () => {
        if (failing) {
          throw new Error("storage did not answer");
        }
      },
    });
    load(5);
    assert.equal(requestsOf(chrome, 5), undefined);
    assert.ok(said.some((line) => line.includes("requests not kept")));
    assert.match(said.join("\n"), /storage did not answer/);

    failing = false;
    await chrome.storage.local.set({ [SETTINGS_KEY]: { list_requests: true } });
    await flush();
    load(6);
    assert.equal(requestsOf(chrome, 6)?.requests.length, 1);

    // A read that fails while requests are being kept stops the keeping and
    // forgets them.
    failing = true;
    await chrome.storage.local.set({ [SETTINGS_KEY]: { list_requests: true } });
    await flush();
    assert.equal(requestsOf(chrome, 6), undefined);
    load(7);
    assert.equal(requestsOf(chrome, 7), undefined);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// Two saves can be read in either order of finishing. The switches as the last
// save left them decide.
test("of two reads of the switches the one begun last decides", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const gates = [];
    let reading = false;
    const { chrome, load } = await workerWatching("last-read", {
      read: () => {
        if (!reading) {
          return undefined;
        }
        return new Promise((resolve) => gates.push(resolve));
      },
    });
    reading = true;
    // The first save turns the tool off and its read is slow. The second turns
    // it on again and its read finishes first.
    await chrome.storage.local.set({ [SETTINGS_KEY]: { list_requests: false } });
    await chrome.storage.local.set({ [SETTINGS_KEY]: { list_requests: true } });
    await flush();
    assert.equal(gates.length, 4, "two reads of two keys each");
    gates.slice(2).forEach((open) => open());
    await flush();
    gates.slice(0, 2).forEach((open) => open());
    await flush();
    load(5);
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// What was kept is dropped when the permission goes, so turning the group off
// leaves nothing of the pages it watched, and a permission nothing to do with
// it changes nothing.
test("taking webRequest back forgets the requests and stops the listeners, and granting it again keeps requests from the next load", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { chrome, permissions, listening, load } =
      await workerWatching("revoked-requests");
    load(5);
    await permissions.revoke({ permissions: ["history"] });
    await settle(() => false);
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);
    assert.deepEqual(listening(), [1, 1, 1, 1, 1]);

    await permissions.revoke({ permissions: ["webRequest"] });
    assert.ok(await settle(() => requestsOf(chrome, 5) === undefined));
    assert.deepEqual(listening(), [0, 0, 0, 0, 0]);

    await permissions.request({ permissions: ["webRequest"] });
    assert.deepEqual(listening(), [1, 1, 1, 1, 1]);
    assert.equal(requestsOf(chrome, 5), undefined, "a page from before");
    load(5);
    assert.equal(requestsOf(chrome, 5)?.requests.length, 1);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("taking a host back forgets the requests, and they are kept again only while the group is still held", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const first = await workerWatching("revoked-host", { stored: EVERY_SITE });
    first.load(5);
    await first.permissions.revoke({ origins: ["https://*/*"] });
    assert.ok(await settle(() => requestsOf(first.chrome, 5) === undefined));
    assert.ok(await settle(() => first.listening().every((n) => n === 0)));

    // Where the group is still held after the host went, requests are kept
    // again, from the next load.
    const second = await workerWatching("kept-host", { stored: EVERY_SITE });
    second.load(5);
    second.permissions.contains = async () => true;
    await second.permissions.revoke({ origins: ["https://*/*"] });
    assert.ok(await settle(() => requestsOf(second.chrome, 5) === undefined));
    assert.ok(await settle(() => second.listening().every((n) => n === 1)));
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A browser that did not have the permission has no webRequest until it is
// granted, and a worker that is running then starts to keep requests.
test("a worker started without webRequest keeps requests once it is granted", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { chrome, permissions, events, listening } = await workerWatching(
      "granted-later",
      { held: {}, webRequest: false },
    );
    assert.deepEqual(listening(), [0, 0, 0, 0, 0]);
    chrome.webRequest = events;
    await permissions.request({ permissions: ["webRequest"] });
    assert.deepEqual(listening(), [1, 1, 1, 1, 1]);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a check of what is held that fails after a host is taken back is reported", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const { permissions } = await workerWatching("check-fails");
    permissions.contains = async () => {
      throw new Error("the browser did not answer");
    };
    await permissions.revoke({ origins: ["https://*/*"] });
    assert.ok(
      await settle(() => said.some((line) => line.includes("not watched"))),
    );
    assert.match(said.join("\n"), /the browser did not answer/);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A worker whose toolbar a test reads, and whose stored values are `stored`.
// `shown` is what the toolbar shows and `press` uses a keyboard command.
async function workerWithToolbar(tag, { stored = {}, held = allOptional() } = {}) {
  const calls = [];
  const permissions = fakePermissions({ held, calls });
  const toolbar = fakeToolbar();
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    ...CONFIRMATION_RUNTIME,
    onInstalled: event(),
    connectNative: () => port(),
  };
  const alarms = { onAlarm: event(), create() {} };
  const kept = storage(stored);
  globalThis.chrome = {
    ...toolbar.api,
    runtime,
    alarms,
    permissions,
    storage: kept,
  };
  await import(`../background.js?${tag}=${Date.now()}`);
  return { stored, calls, permissions, runtime, alarms, toolbar, kept };
}

// A new object each time, since a worker saves into the object it is given.
const twoOn = () => ({
  [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
  [SETTINGS_KEY]: { list_tabs: true, search_history: true },
});

test("the badge counts the groups that are on and its title lists them", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar } = await workerWithToolbar("badge-two", { stored: twoOn() });
    assert.ok(await settle(() => toolbar.shown.text !== ""), "the badge was not drawn");
    assert.equal(toolbar.shown.text, "2");
    assert.equal(toolbar.shown.title, "BraveBot: 2 groups are on: tabs-read, history-read");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The platform check is on whenever the extension is, so counting it would put
// a badge on an extension nobody turned anything on in.
test("the badge is empty and its title says no group is on until the person turns a group on", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar } = await workerWithToolbar("badge-none");
    assert.ok(await settle(() => toolbar.shown.title !== ""), "the badge was not drawn");
    assert.equal(toolbar.shown.text, "");
    assert.equal(toolbar.shown.title, "BraveBot: no group is on");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("the badge counts one group in the singular", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar } = await workerWithToolbar("badge-one", {
      stored: { [GROUP_SWITCHES_KEY]: { "page-read": true } },
    });
    assert.ok(await settle(() => toolbar.shown.text !== ""));
    assert.equal(toolbar.shown.text, "1");
    assert.equal(toolbar.shown.title, "BraveBot: 1 group is on: page-read");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// A group is counted while the person turned it on, its lifetime has not
// ended and the browser holds its permissions: the same test the options page
// draws its switches by.
test("the badge counts only groups that are on, within their lifetime and held", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const held = allOptional();
    held.permissions = held.permissions.filter((name) => name !== "bookmarks");
    const { toolbar } = await workerWithToolbar("badge-held", {
      held,
      stored: {
        [GROUP_SWITCHES_KEY]: {
          "tabs-read": true,
          "history-read": true,
          "bookmarks-read": true,
        },
        [LIFETIMES_KEY]: {
          "history-read": { lifetime: "one-hour", expires: Date.now() - 1 },
        },
      },
    });
    assert.ok(await settle(() => toolbar.shown.text !== ""));
    assert.equal(toolbar.shown.text, "1");
    assert.match(toolbar.shown.title, /: tabs-read$/);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("the badge follows a switch that is saved and a permission that is taken back or granted", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar, kept, permissions } = await workerWithToolbar("badge-follows", {
      stored: twoOn(),
    });
    assert.ok(await settle(() => toolbar.shown.text === "2"));

    await kept.local.set({ [GROUP_SWITCHES_KEY]: { "tabs-read": true } });
    assert.ok(await settle(() => toolbar.shown.text === "1"), "a switch turned off");

    await kept.local.set({
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "bookmarks-read": true },
    });
    assert.ok(await settle(() => toolbar.shown.text === "2"), "a switch turned on");

    // The person takes bookmarks back in the browser's settings.
    await permissions.revoke({ permissions: ["bookmarks"] });
    assert.ok(await settle(() => toolbar.shown.text === "1"), "a permission taken back");
    assert.match(toolbar.shown.title, /: tabs-read$/);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The worker turns off a group whose permission was taken back and the save
// draws the badge. When the save fails the group stays saved as on, and the
// options page draws it off, so the badge does too.
test("the badge drops a group whose permission was taken back even when the worker cannot save it off", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const { toolbar, kept, permissions } = await workerWithToolbar("badge-unsaved", {
      stored: twoOn(),
    });
    assert.ok(await settle(() => toolbar.shown.text === "2"));
    kept.local.set = async () => {
      throw new Error("the storage is full");
    };
    await permissions.revoke({ permissions: ["history"] });
    assert.ok(await settle(() => toolbar.shown.text === "1"), "the badge kept the group");
    assert.ok(await settle(() => said.some((line) => /storage is full/.test(line))));
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

test("the badge follows a lifetime that is saved and the sites a group that reads pages may reach", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar, kept, permissions } = await workerWithToolbar("badge-lifetime", {
      held: { permissions: ["tabs", "scripting", "webNavigation"] },
      stored: { [GROUP_SWITCHES_KEY]: { "page-read": true } },
    });
    // No site is allowed, so the group needs no host and is held.
    assert.ok(await settle(() => toolbar.shown.text === "1"));

    await kept.local.set({
      [LIFETIMES_KEY]: { "page-read": { lifetime: "one-hour", expires: Date.now() - 1 } },
    });
    assert.ok(await settle(() => toolbar.shown.text === ""), "a lifetime that ended");

    await kept.local.set({ [LIFETIMES_KEY]: {} });
    assert.ok(await settle(() => toolbar.shown.text === "1"));

    // An allowed site is a host the browser does not hold yet.
    await kept.local.set({ [ALLOWED_SITES_KEY]: ["https://news.example"] });
    assert.ok(await settle(() => toolbar.shown.text === ""), "a site not held");
    await permissions.request({ origins: ["https://news.example/*"] });
    assert.ok(await settle(() => toolbar.shown.text === "1"), "a site granted");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The log is written by every call. Drawing the badge reads the browser's
// permissions, so a draw for each call would ask it for each call.
test("a call added to the activity log, or a tool switched, does not redraw the badge", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar, kept } = await workerWithToolbar("badge-quiet", { stored: twoOn() });
    assert.ok(await settle(() => toolbar.shown.text === "2"));
    const drawn = toolbar.shown.drawn.length;
    await kept.local.set({ [ACTIVITY_KEY]: [] });
    await kept.local.set({ [SETTINGS_KEY]: { list_tabs: false } });
    await kept.session.set({ [GROUP_SWITCHES_KEY]: {} });
    await settle(() => false);
    assert.equal(toolbar.shown.drawn.length, drawn);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// Two draws can finish in either order, and the one that began first must not
// put what it read before the other's change over the other's.
test("of two draws of the badge the one begun last decides", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar, kept, stored } = await workerWithToolbar("badge-order", {
      stored: twoOn(),
    });
    assert.ok(await settle(() => toolbar.shown.text === "2"));
    const read = kept.local.get;
    let reads = 0;
    let open;
    const gate = new Promise((resolve) => {
      open = resolve;
    });
    kept.local.get = async (key) => {
      if (key !== GROUP_SWITCHES_KEY || (reads += 1) !== 1) {
        return read(key);
      }
      const old = await read(key);
      await gate;
      return old;
    };
    // The first draw reads both groups on and waits. The switches are then
    // turned off, which starts a second draw that finishes first. The change
    // reported is the allowed sites, which only the badge follows.
    const changed = { [ALLOWED_SITES_KEY]: [] };
    kept.onChanged.fire(changed, "local");
    await settle(() => reads === 1);
    stored[GROUP_SWITCHES_KEY] = {};
    kept.onChanged.fire(changed, "local");
    assert.ok(await settle(() => toolbar.shown.text === ""), "the later draw");
    open();
    await settle(() => false);
    assert.equal(toolbar.shown.text, "");
    assert.equal(toolbar.shown.title, "BraveBot: no group is on");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a read of the switches that fails leaves the badge as it was and is reported", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const { toolbar, kept } = await workerWithToolbar("badge-fails", { stored: twoOn() });
    assert.ok(await settle(() => toolbar.shown.text === "2"));
    kept.local.get = async () => {
      throw new Error("the storage is unavailable");
    };
    await kept.onChanged.fire({ [GROUP_SWITCHES_KEY]: {} }, "local");
    assert.ok(await settle(() => said.some((line) => line.includes("badge not drawn"))));
    assert.match(said.join("\n"), /the storage is unavailable/);
    assert.equal(toolbar.shown.text, "2");
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// The keyboard command is declared in the manifest with no key, so the person
// gives it one in the browser. It does what the popup's button does.
test("the keyboard command turns every group off and takes back the optional permissions", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { toolbar, stored, permissions, calls } = await workerWithToolbar("command", {
      stored: twoOn(),
    });
    assert.ok(await settle(() => toolbar.shown.text === "2"));

    toolbar.press("some-other-command");
    await settle(() => false);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], twoOn()[GROUP_SWITCHES_KEY]);

    toolbar.press("turn-everything-off");
    assert.ok(
      await settle(() => Object.keys(stored[GROUP_SWITCHES_KEY]).length === 0),
      "the groups were not turned off",
    );
    assert.equal(stored[SETTINGS_KEY].list_tabs, false);
    assert.equal(stored[SETTINGS_KEY].search_history, false);
    assert.ok(await settle(() => permissions.held.permissions.size === 0));
    assert.deepEqual([...permissions.held.origins], []);
    assert.equal(calls.some(([name]) => name === "permissions.request"), false);
    assert.ok(await settle(() => toolbar.shown.text === ""), "the badge was not cleared");
    assert.equal(toolbar.shown.title, "BraveBot: no group is on");
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a keyboard command that cannot take permissions back is reported, and the groups are off", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    const { toolbar, stored, permissions } = await workerWithToolbar("command-fails", {
      stored: twoOn(),
    });
    permissions.remove = async () => {
      throw new Error("Brave refused");
    };
    toolbar.press("turn-everything-off");
    assert.ok(await settle(() => said.some((line) => line.includes("not all turned off"))));
    assert.match(said.join("\n"), /Brave refused/);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// The held group: the entries the person holds something out with, and the
// items. The worker makes the entries while the group is on, hears the person
// use one as it starts and again when `contextMenus` is granted, and keeps an
// item only while the group is on.
async function workerWithMenus(
  tag,
  { stored = {}, held = allOptional(), absent = false, session = {}, failWith } = {},
) {
  const calls = [];
  const permissions = fakePermissions({ held, calls });
  const kept = storage(stored, session);
  const chrome = {
    ...fakeToolbar().api,
    runtime: {
      lastError: undefined,
      onStartup: event(),
      ...CONFIRMATION_RUNTIME,
      onInstalled: event(),
      connectNative: () => port(),
    },
    alarms: { onAlarm: event(), create() {} },
    permissions,
    storage: kept,
  };
  const menus = withMenus(chrome, { calls, permissions, absentUntilHeld: absent });
  menus.menus.failWith = failWith;
  globalThis.chrome = chrome;
  await import(`../background.js?${tag}=${Date.now()}`);
  return { chrome, calls, permissions, kept, menus, session, stored };
}

const HELD_GROUP_ON = {
  [GROUP_SWITCHES_KEY]: { held: true },
  [SETTINGS_KEY]: { take_held: true },
};
const A_SELECTION = {
  menuItemId: "bravebot-hold-selection",
  selectionText: "what the person selected",
  pageUrl: "https://news.example/story",
};
const ENTRY_IDS = ["bravebot-hold-selection", "bravebot-hold-link", "bravebot-hold-tab"];

test("the worker makes the entries of the context menu as it starts while the held group is on, and none while it is off", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const on = await workerWithMenus("menus-on", {
      stored: { ...HELD_GROUP_ON, [LIFETIMES_KEY]: { held: { lifetime: "until-turned-off" } } },
    });
    assert.ok(await settle(() => on.menus.entries.size === 3), "the entries were not made");
    assert.deepEqual([...on.menus.entries.keys()], ENTRY_IDS);

    const off = await workerWithMenus("menus-off", {
      stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
    });
    await settle(() => false);
    assert.equal(off.menus.entries.size, 0);
    assert.deepEqual(off.menus.calls.filter(([name]) => name.startsWith("contextMenus.")), []);
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("the entries are made when the held group is turned on, and a use of one holds an item while it is on", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { kept, menus } = await workerWithMenus("menus-follow", {
      session: { [RUNS_KEY]: { held: true } },
    });
    await settle(() => false);
    assert.equal(menus.entries.size, 0);

    await kept.local.set(HELD_GROUP_ON);
    assert.ok(await settle(() => menus.entries.size === 3), "the group turned on made no entries");

    menus.click(A_SELECTION);
    assert.ok(
      await settle(() => kept.session.data.held_items?.items.length === 1),
      "a use of an entry held no item",
    );
    assert.equal(kept.session.data.held_items.items[0].text, "what the person selected");

    // The group is turned off in storage and the entry is used before the menu
    // is taken down: it holds nothing.
    await kept.local.set({ [GROUP_SWITCHES_KEY]: {} });
    menus.click({ ...A_SELECTION, selectionText: "after the group was off" });
    await settle(() => false);
    assert.deepEqual(
      kept.session.data.held_items.items.map((item) => item.text),
      ["what the person selected"],
    );
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// take_held returns what is held, so a person who turns it off has nothing held
// for it: the group stays on, the entries stay, and a use of one holds nothing
// until the tool is on again.
test("a use of an entry holds nothing while the take_held switch is off, though its group is on, and holds an item again once it is on", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const { kept, menus } = await workerWithMenus("menus-tool", {
      stored: { ...HELD_GROUP_ON, [LIFETIMES_KEY]: { held: { lifetime: "until-turned-off" } } },
    });
    assert.ok(await settle(() => menus.entries.size === 3), "the entries were not made");

    await kept.local.set({ [SETTINGS_KEY]: { take_held: false } });
    menus.click(A_SELECTION);
    await settle(() => false);
    assert.equal(kept.session.data.held_items, undefined, "an item was held with the tool off");

    await kept.local.set({ [SETTINGS_KEY]: { take_held: true } });
    menus.click(A_SELECTION);
    assert.ok(
      await settle(() => kept.session.data.held_items?.items.length === 1),
      "a use of an entry held no item with the tool on",
    );
  } finally {
    globalThis.chrome = previousChrome;
  }
});

// The API does not exist before the person grants the permission, and the
// browser reports a use of an entry to a listener registered then or at the
// worker's start.
test("a worker started before contextMenus was granted hears the entries once it is", async () => {
  const previousChrome = globalThis.chrome;
  try {
    const held = allOptional();
    held.permissions = held.permissions.filter((name) => name !== "contextMenus");
    const { chrome, permissions, kept, menus } = await workerWithMenus("menus-later", {
      held,
      absent: true,
      stored: { ...HELD_GROUP_ON, [LIFETIMES_KEY]: { held: { lifetime: "until-turned-off" } } },
    });
    await settle(() => false);
    assert.equal(chrome.contextMenus, undefined);
    assert.equal(menus.listeners.length, 0);

    await permissions.request({ permissions: ["contextMenus"] });
    assert.equal(menus.listeners.length, 1);
    assert.ok(await settle(() => menus.entries.size === 3), "the entries were not made");
    menus.click(A_SELECTION);
    assert.ok(await settle(() => kept.session.data.held_items?.items.length === 1));
  } finally {
    globalThis.chrome = previousChrome;
  }
});

test("a menu that cannot be changed is reported and does not stop the worker", async () => {
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  const said = [];
  console.info = (...words) => said.push(words.join(" "));
  try {
    await workerWithMenus("menus-fail", {
      stored: { ...HELD_GROUP_ON, [LIFETIMES_KEY]: { held: { lifetime: "until-turned-off" } } },
      failWith: "the menu is busy",
    });
    await settle(() => said.some((line) => line.includes("the menu is busy")));
    assert.ok(
      said.some((line) => line.includes("bravebot context menu not changed: the menu is busy")),
      said.join("\n"),
    );
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

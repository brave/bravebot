// The popup of the toolbar action against its own markup, with a fake `chrome`
// that stores what is saved and holds the permissions a person has granted.
// The service worker is not loaded: the popup does what it does from the click.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { AWAKE_ALARM, AWAKE_KEY } from "../awake.js";
import { HELD_KEY } from "../held.js";
import { HOUR_MS, LIFETIMES_KEY, ONE_HOUR, RUNS_KEY, recordOf } from "../lifetimes.js";
import { GROUP_SWITCHES_KEY } from "../switches.js";
import { SETTINGS_KEY } from "../tools.js";
import { allOptional } from "./fake-permissions.mjs";
import { fakeChrome } from "./fake-chrome.mjs";

class Element {
  textContent = "";
  disabled = false;
  #listeners = new Map();

  addEventListener(type, listener) {
    this.#listeners.set(type, listener);
  }

  // The person clicking: the listener runs, and the work it starts is awaited.
  async fire(type) {
    const listener = this.#listeners.get(type);
    assert.ok(listener, `nothing listens for ${type} on this control`);
    await listener();
  }
}

const markup = readFileSync(new URL("../popup.html", import.meta.url), "utf8");
const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => id);
const flush = () => new Promise((resolve) => setImmediate(resolve));
let pages = 0;

// Opens the popup over a fake browser. `local`, `session` and `held` are what
// the browser stores and has granted. The fake has the keep_awake API while
// `power` is held, and records its calls with the permission calls.
async function openPopup({ local = {}, session = {}, held = allOptional() } = {}) {
  const elements = new Map(ids.map((id) => [id, new Element()]));
  globalThis.document = {
    getElementById(id) {
      assert.ok(elements.has(id), `popup.html has no element with the id ${id}`);
      return elements.get(id);
    },
  };
  const chrome = fakeChrome({ local, session, held });
  const power = {
    requestKeepAwake: (level) => chrome.calls.push(["power.requestKeepAwake", level]),
    releaseKeepAwake: () => chrome.calls.push(["power.releaseKeepAwake"]),
  };
  Object.defineProperty(chrome, "power", {
    get: () => (chrome.permissions.held.permissions.has("power") ? power : undefined),
  });
  chrome.alarms = {
    async clear(name) {
      chrome.calls.push(["alarms.clear", name]);
      return true;
    },
  };
  globalThis.chrome = chrome;
  await import(`../popup.js?page=${(pages += 1)}`);
  await flush();
  return {
    chrome,
    on: elements.get("on"),
    held: elements.get("held"),
    button: elements.get("turn-off"),
    status: elements.get("status"),
  };
}

const names = (chrome) => chrome.calls.map(([name]) => name);

const THREE_ON = () => ({
  [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true, "browser-info": true },
  [SETTINGS_KEY]: { list_tabs: true, search_history: true, keep_awake: true },
});

test("the popup lists the groups that are on, and says so when none is", async () => {
  const two = await openPopup({
    local: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
    },
  });
  assert.equal(two.on.textContent, "Groups on: tabs-read, history-read.");

  const none = await openPopup();
  assert.equal(none.on.textContent, "No group is on.");
});

test("the popup says how many items are held out, and nothing when none is or when the count cannot be read", async () => {
  const item = (age) => ({ kind: "tab", page: "https://news.example/", time: Date.now() - age });
  const none = await openPopup();
  assert.equal(none.held.textContent, "");

  const one = await openPopup({
    session: { [HELD_KEY]: { items: [item(0)], forgotten: 0 } },
  });
  assert.equal(one.held.textContent, "1 item is held out for a session to fetch.");

  // An item whose time has passed is not counted.
  const three = await openPopup({
    session: { [HELD_KEY]: { items: [item(0), item(1000), item(60 * 60 * 1000)], forgotten: 0 } },
  });
  assert.equal(three.held.textContent, "2 items are held out for a session to fetch.");

  const damaged = await openPopup({ session: { [HELD_KEY]: "not a record" } });
  assert.equal(damaged.held.textContent, "");
});

// The items are in session storage, which the popup shares with the worker, so
// the button that turns every group off empties them from the popup's own page.
test("the button forgets what was held out, and the popup stops saying so", async () => {
  const popup = await openPopup({
    local: {
      [GROUP_SWITCHES_KEY]: { held: true },
      [SETTINGS_KEY]: { take_held: true },
    },
    session: {
      [RUNS_KEY]: { held: true },
      [HELD_KEY]: {
        items: [{ kind: "tab", page: "https://news.example/", time: Date.now() }],
        forgotten: 0,
      },
    },
  });
  assert.equal(popup.held.textContent, "1 item is held out for a session to fetch.");
  await popup.button.fire("click");
  assert.ok(!(HELD_KEY in popup.chrome.session));
  assert.equal(popup.held.textContent, "");
  assert.equal(popup.chrome.permissions.held.permissions.has("contextMenus"), false);
});

test("a group whose hour has passed, or whose permissions are no longer held, is not listed", async () => {
  const held = allOptional();
  held.permissions = held.permissions.filter((name) => name !== "bookmarks");
  const popup = await openPopup({
    held,
    local: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true, "bookmarks-read": true },
      [LIFETIMES_KEY]: { "history-read": recordOf(ONE_HOUR, Date.now() - 2 * HOUR_MS) },
    },
  });
  assert.equal(popup.on.textContent, "Groups on: tabs-read.");
});

test("the button turns every group off from the click, takes the permissions back and asks the browser for nothing", async () => {
  const popup = await openPopup({
    local: { ...THREE_ON(), allowed_sites: ["https://news.example"] },
  });
  assert.equal(popup.button.disabled, false);
  assert.match(popup.on.textContent, /tabs-read, history-read, browser-info/);

  await popup.button.fire("click");

  const { local } = popup.chrome;
  assert.deepEqual(local[GROUP_SWITCHES_KEY], {});
  assert.equal(local[SETTINGS_KEY].list_tabs, false);
  assert.equal(local[SETTINGS_KEY].search_history, false);
  assert.equal(local[SETTINGS_KEY].keep_awake, false);
  assert.deepEqual([...popup.chrome.permissions.held.permissions], []);
  assert.deepEqual([...popup.chrome.permissions.held.origins], []);
  assert.ok(!names(popup.chrome).includes("permissions.request"));
  assert.equal(popup.on.textContent, "No group is on.");
  assert.equal(
    popup.status.textContent,
    "Every group is off, and Brave holds no optional permission for BraveBot.",
  );
  assert.equal(popup.button.disabled, false);
});

test("the button ends a keep_awake request before it takes the power permission back", async () => {
  const popup = await openPopup({
    local: THREE_ON(),
    session: { [AWAKE_KEY]: { level: "system", until: Date.now() + HOUR_MS } },
  });
  await popup.button.fire("click");
  const order = popup.chrome.calls
    .map(([name, query]) => [name, query])
    .filter(([name]) =>
      ["power.releaseKeepAwake", "alarms.clear", "permissions.remove"].includes(name),
    );
  assert.deepEqual(
    order.map(([name]) => name),
    ["power.releaseKeepAwake", "alarms.clear", "permissions.remove"],
  );
  assert.equal(order[1][1], AWAKE_ALARM);
  assert.ok(!(AWAKE_KEY in popup.chrome.session));
});

test("a refusal by the browser to take a permission back is said, with every group already off", async () => {
  const popup = await openPopup({ local: THREE_ON() });
  popup.chrome.permissions.remove = async () => {
    throw new Error("Brave refused");
  };
  await popup.button.fire("click");
  assert.deepEqual(popup.chrome.local[GROUP_SWITCHES_KEY], {});
  assert.equal(
    popup.status.textContent,
    "Brave reported an error: Brave refused. The line above shows what is on.",
  );
  assert.equal(popup.on.textContent, "No group is on.");
  assert.equal(popup.button.disabled, false);
});

test("the button is disabled while it works, so a second click does not start a second turn-off", async () => {
  const popup = await openPopup({ local: THREE_ON() });
  let finish;
  const remove = popup.chrome.permissions.remove.bind(popup.chrome.permissions);
  popup.chrome.permissions.remove = (query) =>
    new Promise((resolve) => {
      finish = () => resolve(remove(query));
    });
  const clicked = popup.button.fire("click");
  for (let turn = 0; turn < 50 && finish === undefined; turn += 1) {
    await flush();
  }
  assert.equal(popup.button.disabled, true);
  finish();
  await clicked;
  assert.equal(popup.button.disabled, false);
});

test("the popup is a button named Turn everything off and a line for what is on", () => {
  assert.match(markup, /<button type="button" id="turn-off">Turn everything off<\/button>/);
  assert.match(markup, /<p id="on" role="status"><\/p>/);
  assert.match(markup, /<div id="status" class="note" role="status"><\/div>/);
});

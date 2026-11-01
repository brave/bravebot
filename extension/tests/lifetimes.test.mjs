// How long a group stays on: chosen when it is turned on, saved with its
// switch, checked at each call and enforced after the service worker or Brave
// restarts, and turned off by the service worker once it has ended.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { GROUPS, NO_ACCESS } from "../groups.js";
import {
  HOUR_MS,
  LIFETIMES_KEY,
  ONE_HOUR,
  RUNS_KEY,
  UNTIL_RESTART,
  UNTIL_TURNED_OFF,
  defaultLifetime,
  lifetimeEnded,
  runMarkers,
} from "../lifetimes.js";
import {
  GROUP_SWITCHES_KEY,
  applyPreset,
  isOn,
  turnOff,
  turnOffLapsed,
  turnOn,
} from "../switches.js";
import { SETTINGS_KEY, groupSwitches, handle, settings } from "../tools.js";
import { fakeChrome, reached } from "./fake-chrome.mjs";

const START = 1_800_000_000_000;

// Date.now() is read by the code under test, so a test sets the clock instead
// of waiting.
function clockAt(now) {
  mock.timers.enable({ apis: ["Date"], now });
}
const setClock = (now) => mock.timers.setTime(now);
afterEach(() => mock.timers.reset());

const tab = { id: 1, windowId: 1, title: "News", url: "https://news.example/" };
const listTabs = (chrome) => handle({ id: 1, method: "list_tabs" }, chrome);

// A browser in which the person turned `id` on for `lifetime` at the start.
async function turnedOn(id, lifetime, extra = {}) {
  const chrome = fakeChrome({ tabs: [tab], held: {}, ...extra });
  assert.equal(await turnOn(chrome, id, NO_ACCESS, lifetime), true);
  return chrome;
}

test("the default lifetime is until turned off, and until Brave restarts for the eleven groups that act on data or hold page text", () => {
  const restart = new Set([
    "downloads",
    "cookies",
    "remove-data",
    "scripts",
    "input",
    "input-trusted",
    "capture-trusted",
    "console-trusted",
    "upload-trusted",
    "screen",
    "held",
  ]);
  for (const id of Object.keys(GROUPS)) {
    assert.equal(
      defaultLifetime(id),
      restart.has(id) ? UNTIL_RESTART : UNTIL_TURNED_OFF,
      id,
    );
  }
});

test("turning a group on saves the lifetime chosen, an hour as the time it ends, and the default where none is chosen", async () => {
  clockAt(START);
  const forever = await turnedOn("tabs-read", UNTIL_TURNED_OFF);
  assert.deepEqual(forever.local[LIFETIMES_KEY], {
    "tabs-read": { lifetime: UNTIL_TURNED_OFF },
  });
  assert.deepEqual(forever.session[RUNS_KEY], {});

  const hour = await turnedOn("tabs-read", ONE_HOUR);
  assert.deepEqual(hour.local[LIFETIMES_KEY], {
    "tabs-read": { lifetime: ONE_HOUR, expires: START + HOUR_MS },
  });
  assert.deepEqual(hour.session[RUNS_KEY], {});

  const restart = await turnedOn("tabs-read", UNTIL_RESTART);
  assert.deepEqual(restart.local[LIFETIMES_KEY], {
    "tabs-read": { lifetime: UNTIL_RESTART },
  });
  assert.deepEqual(restart.session[RUNS_KEY], { "tabs-read": true });

  const read = await turnedOn("history-read");
  assert.deepEqual(read.local[LIFETIMES_KEY], {
    "history-read": { lifetime: UNTIL_TURNED_OFF },
  });
  const cookies = await turnedOn("cookies");
  assert.deepEqual(cookies.local[LIFETIMES_KEY], {
    cookies: { lifetime: UNTIL_RESTART },
  });
  assert.deepEqual(cookies.session[RUNS_KEY], { cookies: true });
});

test("a lifetime that is not one of the three is refused before the browser is asked", async () => {
  const chrome = fakeChrome({ held: {} });
  await assert.rejects(
    turnOn(chrome, "tabs-read", NO_ACCESS, "a week"),
    /there is no lifetime named a week/,
  );
  assert.deepEqual(chrome.calls, []);
  assert.deepEqual(chrome.local, {});
});

// An alarm may not fire, and the service worker may be stopped: the call itself
// reads the time. The last millisecond of the hour still answers.
test("a call is refused once the hour has passed, with no alarm to fire", async () => {
  clockAt(START);
  const chrome = await turnedOn("tabs-read", ONE_HOUR, { held: undefined });
  assert.equal(chrome.alarms, undefined);

  setClock(START + HOUR_MS - 1);
  const before = await listTabs(chrome);
  assert.equal(before.result.length, 1, before.error?.message);

  setClock(START + HOUR_MS);
  chrome.calls.length = 0;
  const after = await listTabs(chrome);
  assert.equal(
    after.error.message,
    "list_tabs is refused: the tabs-read group was turned on for one hour, and that has ended. Turn it on again in the BraveBot extension's options",
  );
  assert.deepEqual(chrome.calls, [], "the browser was reached");
});

test("a group turned on until Brave restarts runs until the browser restarts, and a service worker restart does not end it", async () => {
  const first = await turnedOn("tabs-read", UNTIL_RESTART, { held: undefined });
  assert.equal((await listTabs(first)).result.length, 1);

  const worker = fakeChrome({ local: first.local, session: first.session, tabs: [tab] });
  assert.equal((await listTabs(worker)).result.length, 1);

  const restarted = fakeChrome({ local: first.local, tabs: [tab] });
  const reply = await listTabs(restarted);
  assert.match(
    reply.error.message,
    /^list_tabs is refused: the tabs-read group was turned on until Brave restarts, and that has ended\./,
  );
  assert.ok(!reached(restarted, "tabs.query"));
});

test("a lifetime that cannot be read ends, and a group with no record has the default of its group", async () => {
  const switches = { [GROUP_SWITCHES_KEY]: { "tabs-read": true } };
  const tools = { [SETTINGS_KEY]: { list_tabs: true } };
  for (const stored of [
    "forever",
    ["tabs-read"],
    7,
    { "tabs-read": null },
    { "tabs-read": "until-turned-off" },
    { "tabs-read": { lifetime: "forever" } },
    { "tabs-read": { lifetime: ONE_HOUR } },
    { "tabs-read": { lifetime: ONE_HOUR, expires: "later" } },
    { "tabs-read": { lifetime: ONE_HOUR, expires: null } },
  ]) {
    const chrome = fakeChrome({
      local: { ...switches, ...tools, [LIFETIMES_KEY]: stored },
      tabs: [tab],
    });
    const reply = await listTabs(chrome);
    assert.match(
      reply.error?.message ?? "ran",
      /^list_tabs is refused: the tabs-read group was turned on .*, and that has ended\./,
      JSON.stringify(stored),
    );
    assert.ok(!reached(chrome, "tabs.query"), JSON.stringify(stored));
  }

  const none = fakeChrome({ local: { ...switches, ...tools }, tabs: [tab] });
  assert.equal((await listTabs(none)).result.length, 1);
  assert.equal(await lifetimeEnded(none, "tabs-read"), null);
  assert.equal(await lifetimeEnded(none, "cookies"), UNTIL_RESTART);
  none.session[RUNS_KEY] = { cookies: true };
  assert.equal(await lifetimeEnded(none, "cookies"), null);
});

// An hour's end is saved as the hour after the moment it was turned on, so an end
// that is later than an hour from now was saved by a clock that has since been
// set back, or was damaged, and does not keep the group on for as long as it says.
test("an hour whose end is more than an hour away, or is not a number, has ended", async () => {
  clockAt(START);
  const ended = async (expires) =>
    lifetimeEnded(
      fakeChrome({
        local: { [LIFETIMES_KEY]: { "tabs-read": { lifetime: ONE_HOUR, expires } } },
      }),
      "tabs-read",
    );
  assert.equal(await ended(START + HOUR_MS), null);
  assert.equal(await ended(START + 1), null);
  assert.equal(await ended(START + HOUR_MS + 1), ONE_HOUR);
  assert.equal(await ended(START + 24 * HOUR_MS), ONE_HOUR);
  assert.equal(await ended(Number.MAX_VALUE), ONE_HOUR);
  assert.equal(await ended(Infinity), ONE_HOUR);
  // A number written as text is not a time.
  assert.equal(await ended(String(START + HOUR_MS)), ONE_HOUR);
  assert.equal(await ended([START + HOUR_MS]), ONE_HOUR);
  assert.equal(await ended(true), ONE_HOUR);
});

test("session storage that cannot be read holds no marker, so a group turned on until Brave restarts has ended", async () => {
  const chrome = fakeChrome({
    local: { [LIFETIMES_KEY]: { cookies: { lifetime: UNTIL_RESTART } } },
    session: { [RUNS_KEY]: { cookies: true } },
  });
  assert.equal(await lifetimeEnded(chrome, "cookies"), null);
  chrome.storage.session.get = async () => {
    throw new Error("session storage is not available");
  };
  assert.equal(await lifetimeEnded(chrome, "cookies"), UNTIL_RESTART);
  assert.deepEqual(await runMarkers(chrome), {});
});

test("a group the page draws as on has a switch, a lifetime that has not ended and its permissions", async () => {
  clockAt(START);
  const chrome = await turnedOn("tabs-read", ONE_HOUR);
  const check = async () =>
    isOn(chrome, "tabs-read", await groupSwitches(chrome), NO_ACCESS);
  assert.equal(await check(), true);
  setClock(START + HOUR_MS);
  assert.equal(await check(), false);
});

// The refusal at the call is what keeps a session out. Turning the group off
// afterwards is what gives the permissions back and draws the switch as off.
test("the service worker turns off a group whose hour has passed, with its tools, and gives back what no group still on lists", async () => {
  clockAt(START);
  const chrome = fakeChrome({ held: {}, tabs: [tab] });
  assert.equal(await turnOn(chrome, "tabs-read", NO_ACCESS, ONE_HOUR), true);
  setClock(START + 1000);
  assert.equal(await turnOn(chrome, "page-read", NO_ACCESS, UNTIL_TURNED_OFF), true);

  chrome.calls.length = 0;
  await turnOffLapsed(chrome);
  assert.deepEqual(
    await groupSwitches(chrome),
    { "tabs-read": true, "page-read": true },
    "nothing had ended",
  );
  assert.deepEqual(chrome.calls, [], "nothing was taken back");

  setClock(START + HOUR_MS);
  chrome.calls.length = 0;
  await turnOffLapsed(chrome);
  assert.deepEqual(await groupSwitches(chrome), { "page-read": true });
  const tools = await settings(chrome);
  assert.equal(tools.list_tabs, false);
  assert.equal(tools.list_windows, false);
  assert.equal(tools.read_page, true);
  assert.deepEqual(Object.keys(chrome.local[LIFETIMES_KEY]), ["page-read"]);
  // tabs and webNavigation stay for page-read; the rest of tabs-read goes back.
  assert.deepEqual(
    chrome.calls.filter(([name]) => name === "permissions.remove"),
    [["permissions.remove", { permissions: ["tabGroups", "sessions"] }]],
  );
});

// bookmarks-write is on until turned off by default, and an hour ends it as it
// ends any group: the call is refused with no bookmark made, and the worker then
// turns the group and its tools off and gives back the permissions that
// bookmarks-read, still on, does not list too.
test("a bookmark is added within the hour of bookmarks-write and refused after it, and the worker then turns the group off", async () => {
  clockAt(START);
  const chrome = fakeChrome({ held: {}, tabs: [tab] });
  chrome.bookmarks = {
    async search() {
      return [];
    },
    async create(details) {
      chrome.calls.push(["bookmarks.create", details]);
      return { id: "n1", title: "", url: details.url, parentId: "2" };
    },
    async get() {
      return [{ id: "2", title: "Other bookmarks" }];
    },
  };
  assert.equal(await turnOn(chrome, "bookmarks-write", NO_ACCESS, ONE_HOUR), true);
  assert.equal(await turnOn(chrome, "bookmarks-read", NO_ACCESS, UNTIL_TURNED_OFF), true);
  const add = () =>
    handle(
      { id: 1, method: "create_bookmark", params: { url: "https://a.example/" } },
      chrome,
    );

  setClock(START + HOUR_MS - 1);
  assert.equal((await add()).result.folder, "Other bookmarks");

  setClock(START + HOUR_MS);
  chrome.calls.length = 0;
  assert.equal(
    (await add()).error.message,
    "create_bookmark is refused: the bookmarks-write group was turned on for one hour, and that has ended. Turn it on again in the BraveBot extension's options",
  );
  assert.ok(!reached(chrome, "bookmarks.create"));

  await turnOffLapsed(chrome);
  assert.deepEqual(await groupSwitches(chrome), { "bookmarks-read": true });
  const tools = await settings(chrome);
  assert.equal(tools.create_bookmark, false);
  assert.equal(tools.search_bookmarks, true);
  // bookmarks and readingList stay, since bookmarks-read lists both.
  assert.deepEqual(
    chrome.calls.filter(([name]) => name === "permissions.remove"),
    [],
  );
});

test("the service worker turns off a group turned on until Brave restarts once the browser has restarted", async () => {
  const before = await turnedOn("tabs-read", UNTIL_RESTART);
  await turnOn(before, "history-read", NO_ACCESS, UNTIL_TURNED_OFF);
  const after = fakeChrome({ local: before.local, held: before.permissions.held });
  await turnOffLapsed(after);
  assert.deepEqual(await groupSwitches(after), { "history-read": true });
  assert.equal(after.local[LIFETIMES_KEY]["tabs-read"], undefined);
  assert.equal((await settings(after)).list_tabs, false);
});

test("a lifetime that cannot be read turns off every group that is on", async () => {
  const chrome = await turnedOn("tabs-read", UNTIL_TURNED_OFF);
  await turnOn(chrome, "history-read", NO_ACCESS, UNTIL_TURNED_OFF);
  chrome.local[LIFETIMES_KEY] = "unreadable";
  await turnOffLapsed(chrome);
  assert.deepEqual(await groupSwitches(chrome), {});
  // The next turning on replaces what could not be read.
  assert.equal(await turnOn(chrome, "tabs-read", NO_ACCESS, UNTIL_TURNED_OFF), true);
  assert.deepEqual(chrome.local[LIFETIMES_KEY], {
    "tabs-read": { lifetime: UNTIL_TURNED_OFF },
  });
});

test("turning a group off forgets its lifetime", async () => {
  clockAt(START);
  const chrome = await turnedOn("tabs-read", UNTIL_RESTART);
  await turnOff(chrome, "tabs-read");
  assert.deepEqual(chrome.local[LIFETIMES_KEY], {});
  assert.deepEqual(chrome.session[RUNS_KEY], {});
});

test("a preset gives a group that was off, or whose hour has passed, the default lifetime, and leaves the lifetime of a group still on", async () => {
  clockAt(START);
  const chrome = fakeChrome({ held: {}, tabs: [tab] });
  await turnOn(chrome, "tabs-read", NO_ACCESS, ONE_HOUR);
  setClock(START + 1000);
  await turnOn(chrome, "history-read", NO_ACCESS, ONE_HOUR);
  setClock(START + HOUR_MS);

  assert.equal(await applyPreset(chrome, "look", NO_ACCESS), true);
  assert.deepEqual(chrome.local[LIFETIMES_KEY], {
    "tabs-read": { lifetime: UNTIL_TURNED_OFF },
    "history-read": { lifetime: ONE_HOUR, expires: START + 1000 + HOUR_MS },
    "bookmarks-read": { lifetime: UNTIL_TURNED_OFF },
  });
  assert.equal((await listTabs(chrome)).result.length, 1);
});

// The session marker is the only thing that ends a group at a restart, so it
// is written before the switch that depends on it.
test("a group turned on until Brave restarts is not saved on when its marker cannot be saved", async () => {
  const chrome = fakeChrome({ held: {} });
  chrome.storage.session.set = async () => {
    throw new Error("session storage is full");
  };
  await assert.rejects(
    turnOn(chrome, "tabs-read", NO_ACCESS, UNTIL_RESTART),
    /session storage is full/,
  );
  assert.equal(chrome.local[GROUP_SWITCHES_KEY], undefined);
});

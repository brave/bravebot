// The activity log: every call that reaches the extension is added to it, with
// when it arrived, the group and tool it named, what came of it and the
// origins of the URLs it named, and nothing of the arguments or the result
// beyond those origins. The calls run through `handle`, against a fake `chrome`
// that stores what is saved.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { mock, test } from "node:test";
import {
  ACTIVITY_KEY,
  ACTIVITY_LIMIT,
  ORIGINS_PER_ENTRY,
  ORIGIN_LENGTH_LIMIT,
  activityLog,
  recordCall,
} from "../activity.js";
import { HOUR_MS, LIFETIMES_KEY, ONE_HOUR, recordOf } from "../lifetimes.js";
import { GROUP_SWITCHES_KEY } from "../switches.js";
import {
  BLOCKED_SITES_KEY,
  LIST_LIMIT,
  SETTINGS_KEY,
  handle,
} from "../tools.js";
import { fakeChrome, reached } from "./fake-chrome.mjs";

const call = (method, params = {}, id = 1) => ({ id, method, params });

// A browser with the tabs-read group on and the tools of it, which is what a
// call of list_tabs and list_frames needs, and nothing else turned on.
function browser({ local = {}, ...rest } = {}) {
  return fakeChrome({
    local: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true },
      [SETTINGS_KEY]: { list_tabs: true, list_frames: true },
      ...local,
    },
    ...rest,
  });
}

// The time a call arrives, which a test sets, so an entry's time can be told
// from the time its answer is ready.
function clock(start) {
  let now = start;
  mock.method(Date, "now", () => now);
  return (to) => {
    now = to;
  };
}

const logOf = (chrome) => activityLog(chrome);

test("a call that runs, one a rule of the extension refuses and one that fails are logged with their group, tool and outcome", async () => {
  const tabs = [{ id: 1, windowId: 1, title: "A", url: "https://a.example/" }];
  const chrome = browser({ tabs });
  const setClock = clock(1_000);
  await handle(call("get_platform_info"), chrome);
  setClock(2_000);
  await handle(call("list_tabs"), chrome);
  setClock(3_000);
  await handle(call("search_history", { text: "x" }), chrome);
  setClock(4_000);
  await handle(call("list_frames", { url: "https://b.example/page" }), chrome);
  mock.restoreAll();
  assert.deepEqual(await logOf(chrome), [
    { time: 1_000, group: "platform", tool: "get_platform_info", outcome: "ran", origins: [] },
    { time: 2_000, group: "tabs-read", tool: "list_tabs", outcome: "ran", origins: [] },
    // The tool's switch is off, so the extension refuses it.
    { time: 3_000, group: "history-read", tool: "search_history", outcome: "refused", origins: [] },
    // No tab is open at that address: the tool was tried and did not complete.
    {
      time: 4_000,
      group: "tabs-read",
      tool: "list_frames",
      outcome: "failed",
      origins: ["https://b.example"],
    },
  ]);
  // The log is kept in local storage, where the options page reads it.
  assert.ok(ACTIVITY_KEY in chrome.local);
  assert.ok(!(ACTIVITY_KEY in chrome.session));
});

// Each rule of the extension that stops a call is a refusal, not a failure: the
// log says the extension declined, as against a tool that went wrong.
test("each rule that stops a call before or while it runs is logged as refused", async () => {
  const BANK = "https://bank.example";
  const ended = {
    [LIFETIMES_KEY]: {
      "tabs-read": recordOf(ONE_HOUR, Date.now() - 2 * HOUR_MS),
    },
  };
  const cases = [
    {
      name: "the tool is switched off",
      chrome: browser({ local: { [SETTINGS_KEY]: { list_tabs: false } } }),
      message: call("list_tabs"),
      group: "tabs-read",
    },
    {
      name: "the group is switched off",
      chrome: browser({ local: { [GROUP_SWITCHES_KEY]: {} } }),
      message: call("list_tabs"),
      group: "tabs-read",
    },
    {
      name: "the group's lifetime has ended",
      chrome: browser({ local: ended }),
      message: call("list_tabs"),
      group: "tabs-read",
    },
    {
      name: "the browser does not hold the group's permissions",
      chrome: browser({ held: {} }),
      message: call("list_tabs"),
      group: "tabs-read",
    },
    {
      name: "the site is blocked",
      chrome: browser({ local: { [BLOCKED_SITES_KEY]: [BANK] } }),
      message: call("list_frames", { url: `${BANK}/login` }),
      group: "tabs-read",
      origins: [BANK],
    },
    {
      name: "the blocked list cannot be read",
      chrome: browser({ local: { [BLOCKED_SITES_KEY]: "not a list" } }),
      message: call("list_frames", { url: `${BANK}/login` }),
      group: "tabs-read",
      origins: [BANK],
    },
    {
      name: "the group is limited to sites the person allowed",
      chrome: browser({
        local: { [GROUP_SWITCHES_KEY]: { "page-read": true }, [SETTINGS_KEY]: { read_page: true } },
      }),
      message: call("read_page", { url: "https://other.example/" }),
      group: "page-read",
      origins: ["https://other.example"],
    },
    {
      name: "there is no such method",
      chrome: browser(),
      message: call("delete_everything"),
      group: null,
      tool: null,
    },
  ];
  for (const { name, chrome, message, group, origins = [], tool } of cases) {
    const reply = await handle(message, chrome);
    assert.ok(reply.error, `${name}: the call is answered with an error`);
    const [entry] = await logOf(chrome);
    assert.equal(entry.outcome, "refused", name);
    assert.equal(entry.group, group, name);
    assert.equal(entry.tool, tool === undefined ? message.method : tool, name);
    assert.deepEqual(entry.origins, origins, name);
  }
});

test("a tool that is tried and does not complete is logged as failed, whether it reports why or the browser throws", async () => {
  const chrome = browser();
  await handle(call("list_frames", { url: "https://a.example/" }), chrome);
  // A call whose arguments are not the tool's is the caller's mistake and not a
  // rule of the extension.
  await handle(call("list_frames", {}), chrome);
  chrome.tabs.query = async () => {
    throw new Error("the browser is busy");
  };
  const reply = await handle(call("list_tabs"), chrome);
  assert.match(reply.error.message, /the browser is busy/);
  assert.deepEqual(
    (await logOf(chrome)).map(({ tool, outcome }) => [tool, outcome]),
    [
      ["list_frames", "failed"],
      ["list_frames", "failed"],
      ["list_tabs", "failed"],
    ],
  );
});

// A person who turned nothing on still sees what a session tried: that is the
// call BROWSER-3 leaves to the switches.
test("a call is logged when nothing is turned on, in the order the calls arrived", async () => {
  const chrome = fakeChrome();
  const methods = ["list_tabs", "read_page", "search_history", "open_tab"];
  for (const [index, method] of methods.entries()) {
    await handle(call(method, { url: "https://a.example/" }, index), chrome);
  }
  const log = await logOf(chrome);
  assert.deepEqual(
    log.map(({ tool, outcome }) => [tool, outcome]),
    methods.map((method) => [method, "refused"]),
  );
});

// What is logged is the five fields. A URL gives its origin and not its path,
// query, fragment or credentials; the other arguments and the result give
// nothing.
test("an entry holds nothing of a call's arguments or result beyond the origins of its URLs", async () => {
  const secret = "https://user:hunter2@a.example:8443/private/path?token=abc#frag";
  const chrome = browser({
    tabs: [
      { id: 1, windowId: 1, title: "Secret Title", url: `${secret}` },
    ],
  });
  await handle(call("list_tabs", { text: "my secret query", title: "My Secret Title" }), chrome);
  await handle(
    call("list_frames", { url: secret, text: "page text", query: "search words", title: "t" }),
    chrome,
  );
  const log = await logOf(chrome);
  assert.equal(log.length, 2);
  for (const entry of log) {
    assert.deepEqual(Object.keys(entry).sort(), ["group", "origins", "outcome", "time", "tool"]);
  }
  assert.deepEqual(log[1].origins, ["https://a.example:8443"]);
  const stored = JSON.stringify(chrome.local[ACTIVITY_KEY]);
  for (const text of [
    "hunter2",
    "user:",
    "private",
    "token",
    "frag",
    "secret",
    "Secret",
    "page text",
    "search words",
  ]) {
    assert.ok(!stored.includes(text), `the log holds ${text}`);
  }
});

// The group that changes bookmarks is logged as the others are: its calls are
// the ones a person most wants to see afterwards, and the title and folder a
// session wrote are not kept, only the origin of the address.
test("a bookmark added, and a call to the group while it is off, are logged with the group, the tool and the origin of the address, and nothing of the title or folder", async () => {
  const address = "https://user:hunter2@a.example:8443/private/path?token=abc#frag";
  const write = (chrome) => {
    chrome.bookmarks = {
      async search() {
        return [{ id: "10", title: "Hunter2 folder" }];
      },
      async getTree() {
        return [{ id: "0", title: "", children: [] }];
      },
      async create(details) {
        chrome.calls.push(["bookmarks.create", details]);
        return { id: "n1", title: details.title, url: details.url, parentId: "10" };
      },
      async get() {
        return [{ id: "10", title: "Hunter2 folder" }];
      },
    };
    return chrome;
  };
  const on = write(
    browser({
      local: {
        [GROUP_SWITCHES_KEY]: { "bookmarks-write": true },
        [SETTINGS_KEY]: { create_bookmark: true },
      },
    }),
  );
  await handle(
    call("create_bookmark", { url: address, title: "Hunter2 title", folder: "Hunter2 folder" }),
    on,
  );
  // The tool's own switch is off, though its group's is on.
  await handle(call("rename_bookmark", { url: address, title: "Hunter2 title" }), on);
  const off = write(
    browser({ local: { [SETTINGS_KEY]: { create_bookmark: true } } }),
  );
  await handle(call("create_bookmark", { url: address, title: "Hunter2 title" }), off);

  const entry = (outcome, tool) => ({
    group: "bookmarks-write",
    tool,
    outcome,
    origins: ["https://a.example:8443"],
  });
  const without = ({ time, ...rest }) => rest;
  assert.deepEqual((await logOf(on)).map(without), [
    entry("ran", "create_bookmark"),
    entry("refused", "rename_bookmark"),
  ]);
  assert.deepEqual((await logOf(off)).map(without), [
    entry("refused", "create_bookmark"),
  ]);
  for (const chrome of [on, off]) {
    const stored = JSON.stringify(chrome.local[ACTIVITY_KEY]);
    for (const text of ["hunter2", "Hunter2", "private", "token", "frag", "user:"]) {
      assert.ok(!stored.includes(text), `the log holds ${text}`);
    }
  }
  assert.ok(reached(on, "bookmarks.create"));
  assert.ok(!reached(off, "bookmarks.create"));
});

test("the origins of a call are those of its url, frame_url, to and urls, each once, and none for an address without one", async () => {
  const chrome = browser();
  await handle(
    call("list_frames", {
      url: "https://a.example/x",
      frame_url: "https://b.example:8080/y",
      to: "https://e.example/go?token=1#top",
      urls: [
        "https://a.example/z",
        "http://c.example/",
        "not a url",
        5,
        null,
        "about:blank",
        "view-source:https://d.example/p",
      ],
    }),
    chrome,
  );
  const [entry] = await logOf(chrome);
  assert.deepEqual(entry.origins, [
    "https://a.example",
    "https://b.example:8080",
    "https://e.example",
    "http://c.example",
    "https://d.example",
  ]);
});

test("a call that names many sites keeps the first ten origins", async () => {
  const chrome = browser();
  const urls = Array.from({ length: 25 }, (_, index) => `https://site${index}.example/`);
  await handle(call("list_frames", { urls }), chrome);
  const [entry] = await logOf(chrome);
  assert.equal(entry.origins.length, ORIGINS_PER_ENTRY);
  assert.deepEqual(entry.origins, urls.slice(0, ORIGINS_PER_ENTRY).map((url) => url.slice(0, -1)));
});

// A list of 200,000 entries is refused before the origins are read, with a reply
// and a log entry.
test("a call whose list holds more entries than the bound is refused with a reply, logged as failed, and reaches nothing", async () => {
  assert.equal(LIST_LIMIT, 50);
  const tabs = [{ id: 1, windowId: 1, title: "A", url: "https://a.example/" }];
  const chrome = browser({ tabs });
  const urls = Array.from({ length: 200_000 }, (_, index) => `https://s${index}.example/`);
  const reply = await handle(call("list_frames", { url: "https://a.example/", urls }), chrome);
  assert.equal(reply.id, 1);
  assert.equal(reply.error.code, -32602);
  assert.equal(reply.error.message, "urls may hold at most 50 entries");
  assert.ok(!reached(chrome, "tabs.query"));
  const [entry] = await logOf(chrome);
  assert.equal((await logOf(chrome)).length, 1);
  assert.deepEqual(
    { group: entry.group, tool: entry.tool, outcome: entry.outcome, origins: entry.origins },
    { group: "tabs-read", tool: "list_frames", outcome: "failed", origins: [] },
  );

  // Any list argument has the bound, not only `urls`, and an unknown method is
  // answered and logged too.
  const types = Array.from({ length: LIST_LIMIT + 1 }, () => "cache");
  const wide = await handle(call("clear_browsing_data", { since: "x", types }, 2), chrome);
  assert.equal(wide.error.message, "types may hold at most 50 entries");
  const unknown = await handle(call("no_such_method", { urls }, 3), chrome);
  assert.equal(unknown.id, 3);
  assert.equal(unknown.error.code, -32602);
  assert.deepEqual((await logOf(chrome)).map((logged) => logged.outcome), ["failed", "failed", "failed"]);
});

test("a list of exactly the bound is read as before, and its origins are logged", async () => {
  const chrome = browser({ tabs: [] });
  const urls = Array.from({ length: LIST_LIMIT }, (_, index) => `https://s${index}.example/`);
  const reply = await handle(call("list_frames", { urls }), chrome);
  assert.doesNotMatch(reply.error?.message ?? "", /at most/);
  const [entry] = await logOf(chrome);
  assert.equal(entry.origins.length, ORIGINS_PER_ENTRY);
});

// 200 entries of ten origins that a request could make as long as the message
// limit allows would pass the 10 MB quota of storage.local.
test("an origin is stored cut at 255 characters and an entry keeps ten, so 200 entries stay under about 1 MB", async () => {
  assert.equal(ORIGIN_LENGTH_LIMIT, 255);
  const chrome = browser();
  const long = (index) => `https://${String(index).padStart(3, "0")}${"h".repeat(90_000)}.example/`;
  await handle(call("list_frames", { urls: Array.from({ length: 12 }, (_, index) => long(index)) }), chrome);
  const [entry] = await logOf(chrome);
  assert.equal(entry.origins.length, ORIGINS_PER_ENTRY);
  for (const origin of entry.origins) {
    assert.equal(origin.length, ORIGIN_LENGTH_LIMIT);
    assert.ok(origin.startsWith("https://0"));
  }
  assert.ok(JSON.stringify(entry).length < 3_500);
  // Whoever calls recordCall gets the same bounds.
  await recordCall(chrome, {
    time: 1,
    group: null,
    tool: null,
    outcome: "failed",
    origins: Array.from({ length: 30 }, () => "x".repeat(1_000)),
  });
  const written = (await logOf(chrome)).at(-1);
  assert.equal(written.origins.length, ORIGINS_PER_ENTRY);
  assert.ok(written.origins.every((origin) => origin.length === 255));
  assert.ok(ACTIVITY_LIMIT * JSON.stringify(written).length < 1_000_000);
});

test("an entry's time is when the request arrived, not when the answer was ready", async () => {
  const chrome = browser();
  const setClock = clock(10_000);
  chrome.tabs.query = async () => {
    setClock(99_000);
    return [];
  };
  await handle(call("list_tabs"), chrome);
  mock.restoreAll();
  assert.equal((await logOf(chrome))[0].time, 10_000);
});

test("the log keeps the last 200 calls and drops the oldest", async () => {
  const chrome = fakeChrome();
  let now = 0;
  mock.method(Date, "now", () => (now += 1));
  for (let index = 0; index < ACTIVITY_LIMIT + 5; index += 1) {
    await handle(call("get_platform_info"), chrome);
  }
  mock.restoreAll();
  const log = await logOf(chrome);
  assert.equal(ACTIVITY_LIMIT, 200);
  assert.equal(log.length, 200);
  assert.equal(log[0].time, 6);
  assert.equal(log.at(-1).time, 205);
});

// Two calls that finish together each read the log, add themselves and write
// it. Without one-at-a-time, the second write drops the first entry.
test("calls that finish together are all logged", async () => {
  const chrome = fakeChrome();
  const area = chrome.storage.local;
  const get = area.get.bind(area);
  area.get = async (key) => {
    const found = await get(key);
    await new Promise((resolve) => setImmediate(resolve));
    return found;
  };
  await Promise.all(
    Array.from({ length: 30 }, (_, index) =>
      handle(call("get_platform_info", {}, index), chrome),
    ),
  );
  assert.equal((await logOf(chrome)).length, 30);
});

test("a log that cannot be written does not change the answer, and is reported", async () => {
  const chrome = fakeChrome();
  const set = chrome.storage.local.set.bind(chrome.storage.local);
  chrome.storage.local.set = async (items) => {
    if (ACTIVITY_KEY in items) {
      throw new Error("the storage is full");
    }
    return set(items);
  };
  const reported = [];
  mock.method(console, "info", (...message) => reported.push(message.join(" ")));
  const reply = await handle(call("get_platform_info"), chrome);
  mock.restoreAll();
  assert.equal(reply.error, undefined);
  assert.equal(reply.result.os, "mac");
  assert.equal(reported.length, 1);
  assert.match(reported[0], /the storage is full/);
});

test("a stored log that is not a list, or an entry in it that is not an entry, is left out", async () => {
  const good = { time: 5, group: null, tool: null, outcome: "refused", origins: [] };
  const bad = [
    null,
    "text",
    { ...good, outcome: "completed" },
    { ...good, time: "5" },
    // A time no date can be made from would stop the page drawing the log.
    { ...good, time: 8.64e15 + 1 },
    { ...good, time: -8.64e15 - 1 },
    { ...good, time: 1e300 },
    { ...good, origins: [5] },
    { ...good, tool: 7 },
  ];
  const chrome = fakeChrome({ local: { [ACTIVITY_KEY]: [good, ...bad] } });
  assert.deepEqual(await logOf(chrome), [good]);
  chrome.local[ACTIVITY_KEY] = "not a list";
  assert.deepEqual(await logOf(chrome), []);
  await recordCall(chrome, { ...good, time: 6, extra: "kept out" });
  assert.deepEqual(chrome.local[ACTIVITY_KEY], [{ ...good, time: 6 }]);
});

// A reader of the log is the options page, which shows these fields. A group or
// tool the extension does not have, such as an unknown method, is not stored
// as the text the session sent.
test("a method the extension does not have is logged without the name the session sent", async () => {
  const chrome = fakeChrome();
  await handle(call("delete_everything_at_https://a.example"), chrome);
  const [entry] = await logOf(chrome);
  assert.equal(entry.tool, null);
  assert.ok(!JSON.stringify(chrome.local[ACTIVITY_KEY]).includes("delete_everything"));
  // A message without a method or params is still a call that reached the extension.
  await handle({ id: 2 }, chrome);
  await handle(null, chrome);
  assert.equal((await logOf(chrome)).length, 3);
});

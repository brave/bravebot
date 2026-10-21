// The tools as the native host calls them, against a fake `chrome` that
// records each call.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { mock, test } from "node:test";
import {
  BLOCKED_SITES_KEY,
  DEFAULT_RESULTS,
  MAX_RESULTS,
  OPEN_TIMEOUT_MS,
  PAGE_TEXT_LIMIT,
  SETTINGS_KEY,
  TOOLS,
  handle,
  saveBlockedSites,
} from "../tools.js";

// A fake browser: open tabs, history, bookmarks, stored settings, and the text
// each tab's page holds. `calls` records every API reached, so a test can say
// what was never touched.
function browser({
  tabs = [],
  pages = {},
  frames = {},
  tabMovesAfterFrames = {},
  tabMovesAfterScript = {},
  history = [],
  bookmarks = [],
  // What happens to a tab created at a URL: { kind, url, early }. `kind` is
  // "committed" or "error" (a URL with no entry never reports anything), `url`
  // is the page that committed, and `early` reports it before create returns.
  opening = {},
  stored = tabsOn,
} = {}) {
  const calls = [];
  // A copy, so a test that saves settings does not change the next test's.
  const store = { ...stored };
  const listeners = { onCommitted: new Set(), onErrorOccurred: new Set() };
  let nextTabId = 100;
  const currentTabs = new Map(tabs.map((tab) => [tab.id, tab]));
  return {
    calls,
    storage: {
      local: {
        async get(key) {
          calls.push(["storage.get", key]);
          return key in store ? { [key]: store[key] } : {};
        },
        async set(items) {
          calls.push(["storage.set", items]);
          Object.assign(store, items);
        },
      },
    },
    listeners,
    tabs: {
      async create(properties) {
        calls.push(["tabs.create", properties]);
        const tab = { id: nextTabId++, url: "", pendingUrl: properties.url };
        const outcome = opening[properties.url];
        const report = () => {
          const event =
            outcome.kind === "committed" ? "onCommitted" : "onErrorOccurred";
          for (const listener of [...listeners[event]]) {
            // A sub-frame of the same tab commits first. It is not the page.
            listener({ tabId: tab.id, frameId: 3, url: "https://ads.test/" });
            listener({ tabId: tab.id, frameId: 0, url: outcome.url });
          }
        };
        if (outcome?.early) {
          report();
        } else if (outcome) {
          setTimeout(report, 0);
        }
        return tab;
      },
      async remove(tabId) {
        calls.push(["tabs.remove", tabId]);
      },
      async query(filter) {
        calls.push(["tabs.query", filter]);
        return tabs;
      },
      async get(tabId) {
        calls.push(["tabs.get", tabId]);
        return currentTabs.get(tabId);
      },
    },
    webNavigation: {
      onCommitted: {
        addListener: (listener) => listeners.onCommitted.add(listener),
        removeListener: (listener) => listeners.onCommitted.delete(listener),
      },
      onErrorOccurred: {
        addListener: (listener) => listeners.onErrorOccurred.add(listener),
        removeListener: (listener) =>
          listeners.onErrorOccurred.delete(listener),
      },
      async getAllFrames({ tabId }) {
        calls.push(["webNavigation.getAllFrames", tabId]);
        if (tabMovesAfterFrames[tabId]) {
          currentTabs.set(tabId, tabMovesAfterFrames[tabId]);
        }
        return frames[tabId] ?? [];
      },
    },
    runtime: {
      // With a field of its own besides the three the tool passes on, as a
      // later browser could add one.
      async getPlatformInfo() {
        calls.push(["runtime.getPlatformInfo"]);
        return { os: "mac", arch: "arm64", nacl_arch: "arm64", added: "x" };
      },
    },
    scripting: {
      // Runs the function the extension injects, in the page the test gives
      // that tab: at the tab's own URL unless the page says it moved.
      async executeScript(options) {
        const tabId = options.target.tabId;
        const documentId = options.target.documentIds?.[0];
        const frame = (frames[tabId] ?? []).find(
          (candidate) => candidate.documentId === documentId,
        );
        const frameId = frame?.frameId ?? 0;
        calls.push([
          "scripting.executeScript",
          tabId,
          frameId,
          documentId ?? null,
        ]);
        if (tabMovesAfterScript[tabId]) {
          currentTabs.set(tabId, tabMovesAfterScript[tabId]);
        }
        const page = pages[`${tabId}:${frameId}`] ?? pages[tabId];
        if (page instanceof Error) {
          throw page;
        }
        if (page === null) {
          return [{ frameId, documentId }];
        }
        const at =
          page.href ?? frame?.url ?? tabs.find((tab) => tab.id === tabId)?.url;
        return inPage(at, page, calls, () => [
          {
            frameId,
            documentId,
            result: options.func(...(options.args ?? [])),
          },
        ]);
      },
    },
    history: {
      async search(query) {
        calls.push(["history.search", query]);
        return history.slice(0, query.maxResults);
      },
    },
    bookmarks: {
      async search(query) {
        calls.push(["bookmarks.search", query]);
        return bookmarks;
      },
    },
  };
}

// Calls `run` with the page's `location` and `document` as the globals an
// injected function reads, and puts back what was there.
function inPage(href, page, calls, run) {
  const { location, document } = globalThis;
  globalThis.location = { href };
  globalThis.document = {
    title: page.title,
    body: {
      get innerText() {
        calls.push(["document.body.innerText", href]);
        return page.text;
      },
    },
  };
  try {
    return run();
  } finally {
    Object.assign(globalThis, { location, document });
  }
}

function reached(chrome, api) {
  return chrome.calls.some(([name]) => name === api);
}

// The tab tools turned on, as a person turns them on in the options page. The
// fake browser starts with them so, and a test of the defaults stores nothing.
const tabsOn = {
  [SETTINGS_KEY]: {
    list_tabs: true,
    list_frames: true,
    read_page: true,
  },
};

const allowEverything = {
  [SETTINGS_KEY]: { search_history: true, search_bookmarks: true },
};

const tabs = [
  { id: 1, windowId: 10, title: "Brave", url: "https://brave.com/" },
  {
    id: 2,
    windowId: 10,
    title: "Brave search",
    url: "https://brave.com/search",
  },
];

// The limits are the ones the spec states. The other tests size their input
// from these constants, so they would pass at any value.
test("a page is cut at 100,000 characters and a search at 100 results", () => {
  assert.equal(PAGE_TEXT_LIMIT, 100_000);
  assert.equal(MAX_RESULTS, 100);
});

// The host routes a reply by its id, so every reply carries the request's.
test("a reply keeps its id, with a result or an error", async () => {
  const chrome = browser({ tabs });
  const answered = await handle(
    { id: 7, method: "list_tabs", params: {} },
    chrome,
  );
  assert.equal(answered.id, 7);
  assert.ok("result" in answered && !("error" in answered));

  const refused = await handle(
    { id: 8, method: "close_tabs", params: {} },
    chrome,
  );
  assert.equal(refused.id, 8);
  assert.equal(refused.error.code, -32601);
  assert.ok(!("result" in refused));
});

// A name that happens to be on every object is still not a tool.
test("inherited object names are not methods", async () => {
  for (const method of [
    "toString",
    "constructor",
    "__proto__",
    "hasOwnProperty",
  ]) {
    const reply = await handle({ id: 1, method, params: {} }, browser());
    assert.equal(reply.error?.code, -32601, method);
  }
});

// The platform check is what a person runs to see the extension answer, so it
// says what Brave runs on and nothing else: none of the browser's other fields,
// and nothing from a tab, a page, history or bookmarks.
test("get_platform_info says only what Brave runs on", async () => {
  const chrome = browser({ tabs });
  const reply = await handle({ id: 1, method: "get_platform_info" }, chrome);
  assert.deepEqual(reply.result, {
    os: "mac",
    arch: "arm64",
    nacl_arch: "arm64",
  });
  const reached = chrome.calls.map(([name]) => name);
  assert.deepEqual(reached, ["storage.get", "runtime.getPlatformInfo"]);
});

test("list_tabs gives each tab's id, window, title and URL", async () => {
  const reply = await handle({ id: 1, method: "list_tabs" }, browser({ tabs }));
  assert.deepEqual(reply.result, [
    { id: 1, window_id: 10, title: "Brave", url: "https://brave.com/" },
    {
      id: 2,
      window_id: 10,
      title: "Brave search",
      url: "https://brave.com/search",
    },
  ]);
});

// A frame is identified by an exact web URL a person can judge. Browser-owned
// and opaque URLs cannot name what a later read would expose.
test("list_frames gives web URLs in exactly the tab asked for", async () => {
  const chrome = browser({
    tabs,
    frames: {
      1: [
        {
          frameId: 0,
          documentId: "main-one",
          parentFrameId: -1,
          url: "https://brave.com/",
        },
        {
          frameId: 7,
          documentId: "child-one",
          parentFrameId: 0,
          url: "https://child.example/app",
        },
        {
          frameId: 8,
          documentId: "blank-one",
          parentFrameId: 0,
          url: "about:blank",
        },
        {
          frameId: 9,
          documentId: "blob-one",
          parentFrameId: 0,
          url: "blob:https://brave.com/id",
        },
      ],
      2: [
        {
          frameId: 0,
          documentId: "main-two",
          parentFrameId: -1,
          url: "https://brave.com/search",
        },
        {
          frameId: 3,
          documentId: "other-two",
          parentFrameId: 0,
          url: "https://other.example/",
        },
      ],
    },
  });
  const reply = await handle(
    {
      id: 1,
      method: "list_frames",
      params: { url: "https://brave.com/search" },
    },
    chrome,
  );
  assert.deepEqual(reply.result, [
    { url: "https://brave.com/search", top: true },
    { url: "https://other.example/", top: false },
  ]);
  assert.deepEqual(
    chrome.calls.filter(([name]) => name === "webNavigation.getAllFrames"),
    [["webNavigation.getAllFrames", 2]],
  );

  const near = browser({ tabs, frames: { 1: [] } });
  const missed = await handle(
    { id: 2, method: "list_frames", params: { url: "https://brave.com" } },
    near,
  );
  assert.match(missed.error.message, /no open tab is at https:\/\/brave\.com/);
  assert.ok(!reached(near, "webNavigation.getAllFrames"));
});

// The outer tab URL is what the person approved. A tab that moved while its
// frames were listed cannot return the new page's frame URLs.
test("list_frames refuses a tab that moved", async () => {
  const chrome = browser({
    tabs,
    tabMovesAfterFrames: {
      1: { id: 1, url: "https://elsewhere.example/" },
    },
    frames: {
      1: [
        { frameId: 4, documentId: "private", url: "https://private.example/" },
      ],
    },
  });
  const reply = await handle(
    { id: 1, method: "list_frames", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.match(reply.error.message, /left https:\/\/brave\.com\/ before/);
  assert.ok(!JSON.stringify(reply).includes("private.example"));
});

// The outer and frame URLs together route one read. Two frames at the same URL
// cannot be told apart in the approval question, so neither is silently chosen.
test("read_page selects one frame and refuses ambiguity", async () => {
  const child = "https://child.example/app";
  const chrome = browser({
    tabs,
    frames: {
      1: [
        { frameId: 0, documentId: "outer", url: "https://brave.com/" },
        { frameId: 7, documentId: "child", url: child },
      ],
    },
    pages: {
      "1:0": { title: "Brave", text: "outer" },
      "1:7": { title: "Child", text: "inside the child" },
    },
  });
  const reply = await handle(
    {
      id: 1,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: child },
    },
    chrome,
  );
  assert.deepEqual(reply.result, {
    url: "https://brave.com/",
    frame_url: child,
    title: "Child",
    text: "inside the child",
    truncated: false,
  });
  assert.deepEqual(
    chrome.calls.filter(([name]) => name === "scripting.executeScript"),
    [["scripting.executeScript", 1, 7, "child"]],
  );

  const reused = browser({
    tabs,
    frames: {
      1: [
        { frameId: 7, documentId: "old", url: "https://old.example/" },
        { frameId: 8, documentId: "chosen", url: child },
      ],
    },
    pages: {
      "1:7": { title: "Old", text: "wrong document" },
      "1:8": { title: "Chosen", text: "chosen document" },
    },
  });
  const hardened = await handle(
    {
      id: 2,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: child },
    },
    reused,
  );
  assert.equal(hardened.result.text, "chosen document");
  assert.ok(!JSON.stringify(hardened).includes("wrong document"));
  assert.deepEqual(
    reused.calls.filter(([name]) => name === "scripting.executeScript"),
    [["scripting.executeScript", 1, 8, "chosen"]],
  );

  const ambiguous = browser({
    tabs,
    frames: {
      1: [
        { frameId: 7, documentId: "first", url: child },
        { frameId: 8, documentId: "second", url: child },
      ],
    },
    pages: {
      "1:7": { title: "First", text: "first duplicate" },
      "1:8": { title: "Second", text: "second duplicate" },
    },
  });
  const refused = await handle(
    {
      id: 2,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: child },
    },
    ambiguous,
  );
  assert.match(refused.error.message, /more than one frame is at/);
  assert.ok(!reached(ambiguous, "scripting.executeScript"));
});

// The requested frame can disappear or navigate after it is listed. Its text
// is returned only when the document read still has the approved frame URL.
test("read_page refuses a missing or moved frame", async () => {
  const child = "https://child.example/app";
  const missing = browser({ tabs, frames: { 1: [] } });
  const absent = await handle(
    {
      id: 1,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: child },
    },
    missing,
  );
  assert.match(absent.error.message, /no frame in .* is at/);
  assert.ok(!reached(missing, "scripting.executeScript"));

  const moved = browser({
    tabs,
    frames: {
      1: [{ frameId: 7, documentId: "moved", url: child }],
    },
    pages: {
      "1:7": {
        href: "https://elsewhere.example/",
        title: "Elsewhere",
        text: "not approved",
      },
    },
  });
  const left = await handle(
    {
      id: 2,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: child },
    },
    moved,
  );
  assert.match(left.error.message, /frame left .* before it was read/);
  assert.ok(!JSON.stringify(left).includes("not approved"));
  assert.ok(!reached(moved, "document.body.innerText"));
});

// The outer tab can navigate after the frame was found and read. The frame's
// text still cannot be returned under the URL the person approved.
test("read_page refuses a framed read when its outer tab moved", async () => {
  const outer = "https://brave.com/";
  const child = "https://child.example/app";
  const chrome = browser({
    tabs,
    tabMovesAfterScript: {
      1: { id: 1, url: "https://elsewhere.example/" },
    },
    frames: {
      1: [{ frameId: 7, documentId: "child", url: child }],
    },
    pages: { "1:7": { title: "Child", text: "not returned" } },
  });
  const reply = await handle(
    {
      id: 1,
      method: "read_page",
      params: { url: outer, frame_url: child },
    },
    chrome,
  );
  assert.match(reply.error.message, /tab left .* before its frame was read/);
  assert.ok(!JSON.stringify(reply).includes("not returned"));
});

// An optional frame URL still has to be a nonempty string and name web
// content. Invalid arguments reach no frame-navigation or scripting API.
test("read_page validates frame_url before asking for frames", async () => {
  for (const frame_url of ["", null, 5, "about:blank"]) {
    const chrome = browser({ tabs });
    const reply = await handle(
      {
        id: 1,
        method: "read_page",
        params: { url: "https://brave.com/", frame_url },
      },
      chrome,
    );
    assert.equal(reply.error.code, -32602, String(frame_url));
    assert.ok(!reached(chrome, "webNavigation.getAllFrames"));
    assert.ok(!reached(chrome, "scripting.executeScript"));
  }
});

// The URL is what a person approved. A tab whose URL only starts the same is a
// different page and is not read.
test("read_page reads the tab at exactly that URL and no other", async () => {
  const chrome = browser({
    tabs,
    pages: {
      1: { title: "Brave", text: "home" },
      2: { title: "Search", text: "search" },
    },
  });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "https://brave.com/search" } },
    chrome,
  );
  assert.equal(reply.result.text, "search");
  assert.deepEqual(
    chrome.calls.filter(([name]) => name === "scripting.executeScript"),
    [["scripting.executeScript", 2, 0, null]],
  );

  const near = browser({
    tabs,
    pages: { 1: { title: "Brave", text: "home" } },
  });
  const missed = await handle(
    { id: 2, method: "read_page", params: { url: "https://brave.com" } },
    near,
  );
  assert.match(missed.error.message, /no open tab is at https:\/\/brave\.com/);
  assert.ok(!reached(near, "scripting.executeScript"));
});

// A long page is cut on a whole character. A character outside the Basic
// Multilingual Plane takes two code units, and cutting between them would send
// half of one, which the host cannot parse.
test("read_page never ends a cut page on half a character", async () => {
  const text = "x".repeat(PAGE_TEXT_LIMIT - 1) + "\u{1F600}and the rest";
  const chrome = browser({ tabs, pages: { 1: { title: "Wide", text } } });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.equal(reply.result.text, "x".repeat(PAGE_TEXT_LIMIT - 1));
  assert.equal(reply.result.truncated, true);
});

// JSON carries an unpaired surrogate as an escape the host refuses to parse,
// which would leave the call waiting for a reply that never comes. Every
// string in an answer is well formed, whichever tool gave it.
test("an answer holds no unpaired surrogate", async () => {
  const broken = "a\ud800b";
  const chrome = browser({
    tabs: [{ id: 1, windowId: 10, title: broken, url: "https://brave.com/" }],
    frames: {
      1: [
        {
          frameId: 4,
          documentId: "broken",
          url: `https://frame.example/${broken}`,
        },
      ],
    },
    pages: { 1: { title: broken, text: `text ${broken}` } },
  });
  for (const [method, params] of [
    ["list_tabs", {}],
    ["list_frames", { url: "https://brave.com/" }],
    ["read_page", { url: "https://brave.com/" }],
  ]) {
    const reply = await handle({ id: 1, method, params }, chrome);
    const sent = JSON.stringify(reply);
    assert.ok(!/\\ud[89ab]/i.test(sent), `${method}: ${sent}`);
    assert.ok(sent.includes("a\ufffdb"), `${method}: ${sent}`);
  }
});

test("read_page cuts a long page at the limit and says it did", async () => {
  const long = "x".repeat(PAGE_TEXT_LIMIT + 5);
  const chrome = browser({ tabs, pages: { 1: { title: "Long", text: long } } });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.equal(reply.result.text.length, PAGE_TEXT_LIMIT);
  assert.equal(reply.result.truncated, true);

  const short = browser({
    tabs,
    pages: { 1: { title: "Short", text: "brief" } },
  });
  const whole = await handle(
    { id: 2, method: "read_page", params: { url: "https://brave.com/" } },
    short,
  );
  assert.equal(whole.result.truncated, false);
});

// A browser page such as brave://settings refuses a script, and that is a
// failure saying so.
test("read_page reports a page the browser will not let it read", async () => {
  const chrome = browser({
    tabs: [
      { id: 3, windowId: 10, title: "Settings", url: "brave://settings/" },
    ],
    pages: { 3: new Error("Cannot access a chrome:// URL") },
  });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "brave://settings/" } },
    chrome,
  );
  assert.match(
    reply.error.message,
    /cannot be read: Cannot access a chrome:\/\/ URL/,
  );
});

// The tab can move between being found and being read, and the page it moved
// to is one the person never approved, so nothing of it comes back.
test("read_page refuses a tab that moved before it was read", async () => {
  const elsewhere = { href: "https://elsewhere.example/", title: "Elsewhere" };
  const chrome = browser({
    tabs,
    pages: { 1: { ...elsewhere, text: "not approved" } },
  });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.match(reply.error.message, /left https:\/\/brave\.com\/ before/);
  assert.ok(!("result" in reply));
  assert.ok(!JSON.stringify(reply).includes("not approved"));
  assert.ok(!JSON.stringify(reply).includes("elsewhere.example"));
});

// A script that comes back with nothing did not read the page, and an empty
// result would say the page has no text.
test("read_page fails where the script returned nothing", async () => {
  const chrome = browser({ tabs, pages: { 1: null } });
  const reply = await handle(
    { id: 1, method: "read_page", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.match(reply.error.message, /returned nothing/);
  assert.ok(!("result" in reply));
});

test("read_page refuses a call with no URL", async () => {
  const reply = await handle(
    { id: 1, method: "read_page", params: {} },
    browser({ tabs }),
  );
  assert.equal(reply.error.code, -32602);
});

// The history API searches the last 24 hours unless told a start time. Leaving
// it out would answer a search of all history with one day of it.
test("search_history searches all of history, not the last day", async () => {
  const chrome = browser({ stored: allowEverything });
  await handle(
    { id: 1, method: "search_history", params: { query: "rust" } },
    chrome,
  );
  const [, query] = chrome.calls.find(([name]) => name === "history.search");
  assert.equal(query.startTime, 0);
  assert.equal(query.text, "rust");
});

test("a search keeps its result count between 1 and 100", async () => {
  const asked = async (max_results) => {
    const chrome = browser({ stored: allowEverything });
    await handle(
      { id: 1, method: "search_history", params: { query: "q", max_results } },
      chrome,
    );
    return chrome.calls.find(([name]) => name === "history.search")[1]
      .maxResults;
  };
  assert.equal(await asked(undefined), DEFAULT_RESULTS);
  assert.equal(await asked(5), 5);
  assert.equal(await asked(0), 1);
  assert.equal(await asked(MAX_RESULTS + 1), MAX_RESULTS);
  assert.equal(await asked(Number.POSITIVE_INFINITY), DEFAULT_RESULTS);
});

test("bookmarks leave folders out and keep the result bound", async () => {
  const bookmarks = [
    { id: "folder", title: "Work" },
    ...Array.from({ length: MAX_RESULTS + 5 }, (_, index) => ({
      id: String(index),
      title: `Rust ${index}`,
      url: `https://rust-lang.org/${index}`,
    })),
  ];
  const chrome = browser({ stored: allowEverything, bookmarks });
  const reply = await handle(
    {
      id: 1,
      method: "search_bookmarks",
      params: { query: "r", max_results: MAX_RESULTS + 1 },
    },
    chrome,
  );
  assert.equal(reply.result.length, MAX_RESULTS);
  assert.ok(reply.result.every((bookmark) => "url" in bookmark));
});

// Only the platform check starts on. Every other tool reaches what a person
// has open, visited or saved, so each is refused until they turn it on, and a
// tool that is off is refused before the browser is asked anything.
test("only the platform check starts on", async () => {
  const nothingStored = browser({ tabs, stored: {} });
  const check = await handle(
    { id: 1, method: "get_platform_info" },
    nothingStored,
  );
  assert.ok("result" in check);

  for (const [method, params, api] of [
    ["list_tabs", {}, "tabs.query"],
    ["list_frames", { url: tabs[0].url }, "webNavigation.getAllFrames"],
    ["read_page", { url: tabs[0].url }, "scripting.executeScript"],
    ["search_history", { query: "bank" }, "history.search"],
    ["search_bookmarks", { query: "bank" }, "bookmarks.search"],
    ["open_tab", { url: "https://brave.com/" }, "tabs.create"],
  ]) {
    const off = browser({ tabs, stored: {} });
    const refused = await handle({ id: 1, method, params }, off);
    assert.match(refused.error.message, /turned off/, method);
    assert.ok(!reached(off, api), method);
    assert.ok(!reached(off, "tabs.query"), method);

    const on = browser({
      tabs,
      pages: { 1: { title: "Brave", text: "home" } },
      opening: {
        "https://brave.com/": { kind: "committed", url: "https://brave.com/" },
      },
      stored: { [SETTINGS_KEY]: { [method]: true } },
    });
    const answered = await handle({ id: 1, method, params }, on);
    assert.ok("result" in answered, method);
    assert.ok(reached(on, api), method);
  }
});

test("the platform check can be turned off too", async () => {
  const chrome = browser({
    stored: { [SETTINGS_KEY]: { get_platform_info: false } },
  });
  const off = await handle({ id: 1, method: "get_platform_info" }, chrome);
  assert.match(off.error.message, /turned off/);
  assert.ok(!reached(chrome, "runtime.getPlatformInfo"));
});

test("open tabs are refused once a person turns them off again", async () => {
  const on = await handle({ id: 1, method: "list_tabs" }, browser({ tabs }));
  assert.ok("result" in on);

  for (const method of ["list_tabs", "list_frames", "read_page"]) {
    const chrome = browser({
      tabs,
      stored: { [SETTINGS_KEY]: { [method]: false } },
    });
    const params = method === "list_tabs" ? {} : { url: tabs[0].url };
    const off = await handle({ id: 1, method, params }, chrome);
    assert.match(off.error.message, /turned off/);
    assert.ok(!reached(chrome, "tabs.query"));
    assert.ok(!reached(chrome, "scripting.executeScript"));
  }
});

// open_tab is a request to a site with the person's cookies, so it opens
// exactly the URL they approved and reports only where the tab landed.
test("open_tab opens the URL in a background tab and reports where it loaded", async () => {
  const url = "https://brave.com/search?q=a";
  for (const early of [false, true]) {
    const chrome = browser({
      opening: { [url]: { kind: "committed", url, early } },
      stored: { [SETTINGS_KEY]: { open_tab: true } },
    });
    const reply = await handle(
      { id: 1, method: "open_tab", params: { url } },
      chrome,
    );
    assert.deepEqual(reply.result, { url }, `early: ${early}`);
    assert.deepEqual(
      chrome.calls.filter(([name]) => name.startsWith("tabs.")),
      [["tabs.create", { url, active: false }]],
    );
    assert.equal(chrome.listeners.onCommitted.size, 0);
    assert.equal(chrome.listeners.onErrorOccurred.size, 0);
  }
});

// A same-host redirect is ordinary: the reply names the page that committed.
test("open_tab keeps a tab that redirected within the approved host", async () => {
  const url = "https://brave.com/old";
  const landed = "https://brave.com/new";
  const chrome = browser({
    opening: { [url]: { kind: "committed", url: landed } },
    stored: { [SETTINGS_KEY]: { open_tab: true } },
  });
  const reply = await handle(
    { id: 1, method: "open_tab", params: { url } },
    chrome,
  );
  assert.deepEqual(reply.result, { url: landed });
  assert.ok(!reached(chrome, "tabs.remove"));
});

// The approval named one host. A tab that commits on another is closed, and
// the failure names the URL that was asked for, not the one it went to.
test("open_tab closes a tab that committed on another host", async () => {
  const url = "https://brave.com/go";
  for (const landed of [
    "https://evil.test/",
    "https://brave.com.evil.test/",
    "https://brave.com:8443/",
    "https://sub.brave.com/",
  ]) {
    const chrome = browser({
      opening: { [url]: { kind: "committed", url: landed } },
      stored: { [SETTINGS_KEY]: { open_tab: true } },
    });
    const reply = await handle(
      { id: 1, method: "open_tab", params: { url } },
      chrome,
    );
    assert.ok(!("result" in reply), landed);
    assert.ok(reply.error.message.includes(url), landed);
    assert.ok(!reply.error.message.includes(landed), landed);
    assert.deepEqual(
      chrome.calls.filter(([name]) => name === "tabs.remove"),
      [["tabs.remove", 100]],
      landed,
    );
  }
});

// A page that commits on the approved host but at a non-web URL (an error
// page, say) is not a page the person approved either.
test("open_tab closes a tab that committed on a browser error page", async () => {
  const url = "https://brave.com/";
  const chrome = browser({
    opening: {
      [url]: { kind: "committed", url: "chrome-error://chromewebdata/" },
    },
    stored: { [SETTINGS_KEY]: { open_tab: true } },
  });
  const reply = await handle(
    { id: 1, method: "open_tab", params: { url } },
    chrome,
  );
  assert.ok("error" in reply);
  assert.ok(reply.error.message.includes(url));
  assert.ok(!reply.error.message.includes("chrome-error"));
  assert.ok(reached(chrome, "tabs.remove"));
});

test("open_tab closes a tab whose page failed to load", async () => {
  const url = "https://brave.test/";
  const chrome = browser({
    opening: { [url]: { kind: "error", url } },
    stored: { [SETTINGS_KEY]: { open_tab: true } },
  });
  const reply = await handle(
    { id: 1, method: "open_tab", params: { url } },
    chrome,
  );
  assert.match(reply.error.message, /failed to load/);
  assert.ok(reply.error.message.includes(url));
  assert.ok(reached(chrome, "tabs.remove"));
});

// A page that never commits is answered before the host gives up on the
// extension, and the tab it opened is not left behind.
test("open_tab gives up on a page that never commits and closes the tab", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const url = "https://hang.test/";
    const chrome = browser({ stored: { [SETTINGS_KEY]: { open_tab: true } } });
    const pending = handle(
      { id: 1, method: "open_tab", params: { url } },
      chrome,
    );
    // Let the call reach its wait, then run the clock out.
    for (let turn = 0; turn < 10; turn += 1) {
      await Promise.resolve();
    }
    assert.ok(!reached(chrome, "tabs.remove"));
    // The limit is twenty seconds: a tab still loading a millisecond short of
    // it is kept waiting, and the millisecond after it is closed.
    assert.equal(OPEN_TIMEOUT_MS, 20_000);
    mock.timers.tick(19_999);
    for (let turn = 0; turn < 10; turn += 1) {
      await Promise.resolve();
    }
    assert.ok(!reached(chrome, "tabs.remove"));
    mock.timers.tick(1);
    const reply = await pending;
    assert.match(reply.error.message, /did not load in time/);
    assert.ok(reply.error.message.includes(url));
    assert.ok(reached(chrome, "tabs.remove"));
    assert.equal(chrome.listeners.onCommitted.size, 0);
  } finally {
    mock.timers.reset();
  }
});

// The tab's own page is the top frame: a frame inside it committing on another
// host is not the page that loaded, and one tab's events are not another's.
test("open_tab reads only its own tab's top-frame commit", async () => {
  const url = "https://brave.com/";
  const chrome = browser({
    opening: { [url]: { kind: "committed", url } },
    stored: { [SETTINGS_KEY]: { open_tab: true } },
  });
  const create = chrome.tabs.create;
  chrome.tabs.create = async (properties) => {
    // Another tab commits somewhere else first.
    for (const listener of [...chrome.listeners.onCommitted]) {
      listener({ tabId: 7, frameId: 0, url: "https://other.test/" });
    }
    return create(properties);
  };
  const reply = await handle(
    { id: 1, method: "open_tab", params: { url } },
    chrome,
  );
  assert.deepEqual(reply.result, { url });
});

// A scheme the person cannot judge from the question, or no URL at all, never
// reaches the browser.
test("open_tab refuses a URL that is not HTTP or HTTPS without opening a tab", async () => {
  for (const url of [
    "file:///etc/passwd",
    "javascript:alert(1)",
    "data:text/html,hi",
    "chrome://settings/passwords",
    "about:blank",
    "brave.com",
    " ",
    "",
  ]) {
    const chrome = browser({ stored: { [SETTINGS_KEY]: { open_tab: true } } });
    const reply = await handle(
      { id: 1, method: "open_tab", params: { url } },
      chrome,
    );
    assert.equal(reply.error?.code, -32602, JSON.stringify(url));
    assert.ok(!reached(chrome, "tabs.create"), JSON.stringify(url));
  }
  const chrome = browser({ stored: { [SETTINGS_KEY]: { open_tab: true } } });
  const missing = await handle(
    { id: 1, method: "open_tab", params: {} },
    chrome,
  );
  assert.equal(missing.error.code, -32602);
  assert.ok(!reached(chrome, "tabs.create"));
});

// The blocked sites a person wrote in the options page, with every tool they
// could be read, listed or searched through turned on.
const everythingOn = {
  [SETTINGS_KEY]: {
    list_tabs: true,
    list_frames: true,
    read_page: true,
    search_history: true,
    search_bookmarks: true,
    open_tab: true,
  },
};

const bank = "https://bank.example";

function blocking(sites, settings = everythingOn) {
  return { ...settings, [BLOCKED_SITES_KEY]: sites };
}

// A refusal for a blocked site is the same words whatever was asked about, and
// holds no part of the URL, since the failure goes back to the session.
const BLOCKED_MESSAGE =
  "that site is in the blocked sites list in the BraveBot extension's options";

// A site is blocked by its origin as written. Every spelling of a URL at that
// origin is covered, and a URL that only resembles the origin is not: a
// matcher on the host, a suffix or a substring would block or pass the wrong
// rows of this table.
test("a blocked site covers every URL at its origin and no other", async () => {
  const at = [
    `${bank}/`,
    `${bank}/login?next=%2Faccount#top`,
    "HTTPS://BANK.EXAMPLE:443/path",
    "https://user:secret@bank.example/",
    "blob:https://bank.example/7d1c",
    "view-source:https://bank.example/",
    "VIEW-SOURCE:https://bank.example/",
  ];
  const apart = [
    "https://www.bank.example/",
    "http://bank.example/",
    "https://bank.example:8443/",
    "https://notbank.example/",
    "https://bank.example.evil.test/",
    "https://evil.test/?to=https://bank.example",
    "https://evil.test/https://bank.example/",
    "about:blank",
    "chrome://settings/",
    "",
  ];
  const rows = [...at, ...apart].map((url, index) => ({
    id: index + 1,
    windowId: 1,
    title: `tab ${index}`,
    url,
  }));
  const reply = await handle(
    { id: 1, method: "list_tabs" },
    browser({ tabs: rows, stored: blocking([bank]) }),
  );
  assert.deepEqual(
    reply.result.map((tab) => tab.url),
    apart,
  );
});

// A tab that has not committed has no URL, and its title is the address it is
// loading. A tab on its way out of a blocked site or into one is left out too.
test("list_tabs leaves out a tab loading a blocked site", async () => {
  const rows = [
    {
      id: 1,
      windowId: 1,
      title: "bank.example/login",
      url: "",
      pendingUrl: `${bank}/login`,
    },
    {
      id: 2,
      windowId: 1,
      title: "Bank",
      url: `${bank}/`,
      pendingUrl: "https://news.example/",
    },
    { id: 3, windowId: 1, title: "News", url: "https://news.example/" },
    {
      id: 4,
      windowId: 1,
      title: "news.example",
      url: "",
      pendingUrl: "https://news.example/",
    },
  ];
  const reply = await handle(
    { id: 1, method: "list_tabs" },
    browser({ tabs: rows, stored: blocking([bank]) }),
  );
  assert.deepEqual(
    reply.result.map((tab) => tab.id),
    [3, 4],
  );
  assert.ok(!JSON.stringify(reply).includes("bank"));
});

// The call is refused before a tab is looked for, so the refusal says nothing
// of whether the person has the site open, and it names the list and not the
// page. A frame at a blocked site is refused the same way inside a tab that
// is not blocked.
test("list_frames and read_page refuse a blocked site, naming the list", async () => {
  const open = [
    { id: 1, windowId: 1, title: "Bank", url: `${bank}/account` },
    { id: 2, windowId: 1, title: "News", url: "https://news.example/" },
  ];
  const messages = new Set();
  for (const [method, params] of [
    ["list_frames", { url: `${bank}/account` }],
    ["read_page", { url: `${bank}/account` }],
    ["list_frames", { url: `${bank}/not-open` }],
    ["read_page", { url: `${bank}/not-open` }],
    [
      "read_page",
      { url: "https://news.example/", frame_url: `${bank}/widget` },
    ],
    [
      "read_page",
      { url: `${bank}/account`, frame_url: "https://ads.example/" },
    ],
  ]) {
    const chrome = browser({
      tabs: open,
      frames: {
        2: [
          { frameId: 0, documentId: "top", url: "https://news.example/" },
          { frameId: 5, documentId: "bank", url: `${bank}/widget` },
          { frameId: 6, documentId: "ads", url: "https://ads.example/" },
        ],
      },
      pages: { 1: { title: "Bank", text: "balance" } },
      stored: blocking([bank]),
    });
    const reply = await handle({ id: 1, method, params }, chrome);
    messages.add(reply.error?.message);
    const asked = `${method} ${JSON.stringify(params)}`;
    assert.ok(!JSON.stringify(reply).includes("bank"), asked);
    for (const api of [
      "tabs.query",
      "webNavigation.getAllFrames",
      "scripting.executeScript",
    ]) {
      assert.ok(!reached(chrome, api), `${method} ${api}`);
    }
  }
  assert.deepEqual([...messages], [BLOCKED_MESSAGE]);
});

// A frame at a blocked site inside a tab that is not blocked is left out of
// the list and cannot be read, while the tab and its other frames still can.
test("a frame at a blocked site is left out and unreadable in an open tab", async () => {
  const chrome = browser({
    tabs,
    frames: {
      1: [
        { frameId: 0, documentId: "top", url: "https://brave.com/" },
        { frameId: 5, documentId: "bank", url: `${bank}/widget` },
        { frameId: 6, documentId: "ads", url: "https://ads.example/" },
      ],
    },
    pages: {
      "1:0": { title: "Brave", text: "home" },
      "1:5": { title: "Bank", text: "balance" },
      "1:6": { title: "Ads", text: "buy now" },
    },
    stored: blocking([bank]),
  });
  const listed = await handle(
    { id: 1, method: "list_frames", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.deepEqual(listed.result, [
    { url: "https://brave.com/", top: true },
    { url: "https://ads.example/", top: false },
  ]);
  const ads = await handle(
    {
      id: 2,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: "https://ads.example/" },
    },
    chrome,
  );
  assert.equal(ads.result.text, "buy now");
  const top = await handle(
    { id: 3, method: "read_page", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.equal(top.result.text, "home");
  assert.ok(!JSON.stringify([listed, ads, top]).includes("balance"));
});

// The newest matches can all be at the blocked site. A search that asked the
// browser for no more than it returns would come back empty then, which reads
// as no history matching.
test("search_history leaves out a blocked site and fills the result from the rest", async () => {
  const history = [
    ...Array.from({ length: 30 }, (_, index) => ({
      title: `Bank ${index}`,
      url: `${bank}/statement/${index}`,
      lastVisitTime: 2_000_000 - index,
    })),
    ...Array.from({ length: 8 }, (_, index) => ({
      title: `News ${index}`,
      url: `https://news.example/${index}`,
      lastVisitTime: 1_000_000 - index,
    })),
  ];
  const chrome = browser({ history, stored: blocking([bank]) });
  const reply = await handle(
    {
      id: 1,
      method: "search_history",
      params: { query: "a", max_results: 5 },
    },
    chrome,
  );
  assert.deepEqual(
    reply.result.map((item) => item.url),
    [0, 1, 2, 3, 4].map((index) => `https://news.example/${index}`),
  );
  assert.ok(!JSON.stringify(reply).includes("bank"));
});

// Bookmarks are not searched in a window of their own: the blocked ones are
// dropped before the count is applied, so the first of them do not use it up.
test("search_bookmarks leaves out a blocked site before it counts the result", async () => {
  const bookmarks = [
    { id: "folder", title: "Work" },
    ...Array.from({ length: 5 }, (_, index) => ({
      id: `b${index}`,
      title: `Bank ${index}`,
      url: `${bank}/${index}`,
    })),
    ...Array.from({ length: 5 }, (_, index) => ({
      id: `n${index}`,
      title: `News ${index}`,
      url: `https://news.example/${index}`,
    })),
  ];
  const reply = await handle(
    {
      id: 1,
      method: "search_bookmarks",
      params: { query: "a", max_results: 3 },
    },
    browser({ bookmarks, stored: blocking([bank]) }),
  );
  assert.deepEqual(
    reply.result.map((bookmark) => bookmark.url),
    [0, 1, 2].map((index) => `https://news.example/${index}`),
  );
});

// open_tab sends the person's cookies to a site, so it does not open a blocked
// one, and it closes a tab that commits at one after a redirect from a site
// that is not blocked. Neither failure names where the tab went.
test("open_tab does not open a blocked site and closes a tab that reaches one", async () => {
  const refused = browser({ stored: blocking([bank]) });
  const reply = await handle(
    { id: 1, method: "open_tab", params: { url: `${bank}/login` } },
    refused,
  );
  assert.equal(reply.error.message, BLOCKED_MESSAGE);
  assert.ok(!reached(refused, "tabs.create"));

  const asked = "http://bank.example/";
  const landed = `${bank}/secret-path`;
  const redirected = browser({
    opening: { [asked]: { kind: "committed", url: landed } },
    stored: blocking([bank]),
  });
  const closed = await handle(
    { id: 2, method: "open_tab", params: { url: asked } },
    redirected,
  );
  assert.equal(closed.error.message, BLOCKED_MESSAGE);
  assert.deepEqual(
    redirected.calls.filter(([name]) => name === "tabs.remove"),
    [["tabs.remove", 100]],
  );
});

// A list that cannot be read is not an empty list. Each tool that reaches a
// site fails and touches no browser API, and the platform check, which reaches
// none, still answers.
test("an unreadable blocked list refuses every tool that reaches a site", async () => {
  for (const unreadable of [bank, { 0: bank }, [bank, 7], [null], 7, false]) {
    for (const [method, params, api] of [
      ["list_tabs", {}, "tabs.query"],
      ["list_frames", { url: tabs[0].url }, "webNavigation.getAllFrames"],
      ["read_page", { url: tabs[0].url }, "scripting.executeScript"],
      ["search_history", { query: "a" }, "history.search"],
      ["search_bookmarks", { query: "a" }, "bookmarks.search"],
      ["open_tab", { url: tabs[0].url }, "tabs.create"],
    ]) {
      const chrome = browser({ tabs, stored: blocking(unreadable) });
      const reply = await handle({ id: 1, method, params }, chrome);
      assert.match(reply.error?.message, /cannot be read/, method);
      assert.ok(!reached(chrome, api), `${method} ${api}`);
      assert.ok(!reached(chrome, "tabs.query"), method);
    }
    const chrome = browser({ stored: blocking(unreadable) });
    const check = await handle({ id: 1, method: "get_platform_info" }, chrome);
    assert.ok("result" in check);
  }
});

// What a person writes becomes origins: a path is dropped, case and a default
// port are normalised the way the browser does, and a site named twice is kept
// once. A line that is not an HTTP or HTTPS address of a host is returned. The
// URL parser takes a wildcard or a comma in a host, and a site saved under one
// would match nothing and leave open the site it was meant to cover.
test("what is written in the options page becomes a list of origins", async () => {
  const saved = await saveBlockedSites(
    browser(),
    [
      "https://Mail.Example.com/inbox?x=1",
      "",
      "  http://intranet:8080  ",
      "https://mail.example.com:443/",
      "http://[::1]:3000/",
      "",
    ].join("\r\n"),
  );
  assert.deepEqual(saved, {
    sites: [
      "https://mail.example.com",
      "http://intranet:8080",
      "http://[::1]:3000",
    ],
    rejected: [],
  });

  for (const line of [
    "bank.example",
    "ftp://files.example",
    "javascript:alert(1)",
    "view-source:https://bank.example",
    "https://*.bank.example",
    "https://a.example,https://b.example",
    "https://",
    "https://bank..example",
    "https://bank example",
    "https://a.example/, https://b.example/",
    "https://a.example/ https://b.example/",
    "https://a.example/;https://b.example/",
    "https://a.example/,https://b.example/",
    "https://a.example\thttps://b.example",
  ]) {
    const { rejected } = await saveBlockedSites(browser(), line);
    assert.deepEqual(rejected, [line], line);
  }
  assert.deepEqual(await saveBlockedSites(browser(), ""), {
    sites: [],
    rejected: [],
  });
});

// What the options page saves is what the tools read: a save blocks the site
// in the next call, a line that is not an address stops the save and leaves the
// list that was saved before in force, and an emptied list blocks nothing again.
test("a saved list blocks the next call and a rejected save changes nothing", async () => {
  const chrome = browser({
    tabs: [
      { id: 1, windowId: 1, title: "Bank", url: `${bank}/` },
      { id: 2, windowId: 1, title: "News", url: "https://news.example/" },
    ],
  });
  const urls = async () =>
    (await handle({ id: 1, method: "list_tabs" }, chrome)).result.map(
      (tab) => tab.url,
    );
  assert.deepEqual(await urls(), [`${bank}/`, "https://news.example/"]);

  await saveBlockedSites(chrome, `${bank}/login\n`);
  assert.deepEqual(await urls(), ["https://news.example/"]);

  // The line for the bank lost its scheme, which would drop the bank if the
  // others were saved.
  const refused = await saveBlockedSites(chrome, "bank.example\n");
  assert.deepEqual(refused.rejected, ["bank.example"]);
  assert.deepEqual(await urls(), ["https://news.example/"]);
  assert.equal(
    chrome.calls.filter(([name]) => name === "storage.set").length,
    1,
  );

  await saveBlockedSites(chrome, "");
  assert.deepEqual(await urls(), [`${bank}/`, "https://news.example/"]);
});

// What a blocked frame embeds is part of what its site shows. A frame inside
// one is left out of the list and is not found by a read, as if it were not
// there, while a frame beside it is still listed.
test("a frame inside a blocked frame is left out and not found", async () => {
  const pixel = "https://analytics.example/pixel?acct=12345";
  const chrome = browser({
    tabs,
    frames: {
      1: [
        {
          frameId: 0,
          parentFrameId: -1,
          documentId: "top",
          url: "https://brave.com/",
        },
        {
          frameId: 5,
          parentFrameId: 0,
          documentId: "bank",
          url: `${bank}/widget`,
        },
        { frameId: 6, parentFrameId: 5, documentId: "deep", url: pixel },
        {
          frameId: 7,
          parentFrameId: 6,
          documentId: "deeper",
          url: "https://cdn.example/",
        },
        {
          frameId: 8,
          parentFrameId: 0,
          documentId: "ads",
          url: "https://ads.example/",
        },
      ],
    },
    pages: {
      "1:6": { title: "Pixel", text: "acct 12345" },
      "1:8": { title: "Ads", text: "buy" },
    },
    stored: blocking([bank]),
  });
  const listed = await handle(
    { id: 1, method: "list_frames", params: { url: "https://brave.com/" } },
    chrome,
  );
  assert.deepEqual(listed.result, [
    { url: "https://brave.com/", top: true },
    { url: "https://ads.example/", top: false },
  ]);
  assert.ok(!JSON.stringify(listed).includes("analytics"));

  for (const frame_url of [pixel, "https://cdn.example/"]) {
    const read = await handle(
      {
        id: 2,
        method: "read_page",
        params: { url: "https://brave.com/", frame_url },
      },
      chrome,
    );
    assert.match(
      read.error.message,
      /no frame in https:\/\/brave\.com\/ is at/,
    );
    assert.ok(!("result" in read), frame_url);
  }
  assert.ok(!reached(chrome, "scripting.executeScript"));

  const ads = await handle(
    {
      id: 3,
      method: "read_page",
      params: { url: "https://brave.com/", frame_url: "https://ads.example/" },
    },
    chrome,
  );
  assert.equal(ads.result.text, "buy");
});

// Each tool decides for itself whether to read the list, so a tool added later
// is open to a blocked site until its author adds a row here. Every tool but
// the platform check, which reaches nothing of the person's, is called with a
// blocked site open in a tab, in history and in bookmarks, and has to return
// nothing of it and reach no tab, frame or script.
const callsNamingTheBank = {
  list_tabs: {},
  list_frames: { url: `${bank}/account` },
  read_page: { url: `${bank}/account` },
  search_history: { query: "a" },
  search_bookmarks: { query: "a" },
  open_tab: { url: `${bank}/login` },
};

test("no tool returns or reaches a blocked site", async () => {
  for (const method of Object.keys(TOOLS)) {
    if (method === "get_platform_info") {
      continue;
    }
    const params = callsNamingTheBank[method];
    assert.ok(params, `${method} has no row naming a blocked site`);
    const chrome = browser({
      tabs: [{ id: 1, windowId: 1, title: "Bank", url: `${bank}/account` }],
      frames: {
        1: [
          {
            frameId: 0,
            parentFrameId: -1,
            documentId: "top",
            url: `${bank}/account`,
          },
        ],
      },
      pages: { 1: { title: "Bank", text: "balance" } },
      history: [{ title: "Bank", url: `${bank}/account` }],
      bookmarks: [{ id: "b", title: "Bank", url: `${bank}/account` }],
      opening: {
        [`${bank}/login`]: { kind: "committed", url: `${bank}/login` },
      },
      stored: blocking([bank]),
    });
    const reply = await handle({ id: 1, method, params }, chrome);
    assert.ok(
      !JSON.stringify(reply.result ?? "").toLowerCase().includes("bank"),
      method,
    );
    for (const api of [
      "tabs.create",
      "webNavigation.getAllFrames",
      "scripting.executeScript",
    ]) {
      assert.ok(!reached(chrome, api), `${method} reached ${api}`);
    }
  }
});

// The tools as the native host calls them, against a fake `chrome` that
// records each call.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  DEFAULT_RESULTS,
  MAX_RESULTS,
  PAGE_TEXT_LIMIT,
  SETTINGS_KEY,
  handle,
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
  stored = tabsOn,
} = {}) {
  const calls = [];
  const currentTabs = new Map(tabs.map((tab) => [tab.id, tab]));
  return {
    calls,
    storage: {
      local: {
        async get(key) {
          calls.push(["storage.get", key]);
          return key in stored ? { [key]: stored[key] } : {};
        },
      },
    },
    tabs: {
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
  ]) {
    const off = browser({ tabs, stored: {} });
    const refused = await handle({ id: 1, method, params }, off);
    assert.match(refused.error.message, /turned off/, method);
    assert.ok(!reached(off, api), method);
    assert.ok(!reached(off, "tabs.query"), method);

    const on = browser({
      tabs,
      pages: { 1: { title: "Brave", text: "home" } },
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

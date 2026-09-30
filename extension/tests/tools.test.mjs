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
  history = [],
  bookmarks = [],
  stored = {},
} = {}) {
  const calls = [];
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
    },
    scripting: {
      // Runs the function the extension injects, in the page the test gives
      // that tab: at the tab's own URL unless the page says it moved.
      async executeScript(options) {
        const id = options.target.tabId;
        calls.push(["scripting.executeScript", id]);
        const page = pages[id];
        if (page instanceof Error) {
          throw page;
        }
        if (page === null) {
          return [{ frameId: 0 }];
        }
        const at = page.href ?? tabs.find((tab) => tab.id === id)?.url;
        return inPage(at, page, () => [{ frameId: 0, result: options.func() }]);
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
function inPage(href, page, run) {
  const { location, document } = globalThis;
  globalThis.location = { href };
  globalThis.document = { title: page.title, body: { innerText: page.text } };
  try {
    return run();
  } finally {
    Object.assign(globalThis, { location, document });
  }
}

function reached(chrome, api) {
  return chrome.calls.some(([name]) => name === api);
}

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
    [["scripting.executeScript", 2]],
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
    pages: { 1: { title: broken, text: `text ${broken}` } },
  });
  for (const [method, params] of [
    ["list_tabs", {}],
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

// History and bookmarks reach a person's whole past, so they start off. A
// tool that is off is refused before the browser is asked anything.
test("searches start off, and an off tool touches nothing", async () => {
  for (const [method, api] of [
    ["search_history", "history.search"],
    ["search_bookmarks", "bookmarks.search"],
  ]) {
    const off = browser();
    const refused = await handle(
      { id: 1, method, params: { query: "bank" } },
      off,
    );
    assert.match(refused.error.message, /turned off/, method);
    assert.ok(!reached(off, api), method);

    const on = browser({ stored: { [SETTINGS_KEY]: { [method]: true } } });
    const answered = await handle(
      { id: 1, method, params: { query: "bank" } },
      on,
    );
    assert.ok("result" in answered, method);
    assert.ok(reached(on, api), method);
  }
});

test("open tabs work until a person turns them off", async () => {
  const on = await handle({ id: 1, method: "list_tabs" }, browser({ tabs }));
  assert.ok("result" in on);

  for (const method of ["list_tabs", "read_page"]) {
    const chrome = browser({
      tabs,
      stored: { [SETTINGS_KEY]: { [method]: false } },
    });
    const params = method === "read_page" ? { url: tabs[0].url } : {};
    const off = await handle({ id: 1, method, params }, chrome);
    assert.match(off.error.message, /turned off/);
    assert.ok(!reached(chrome, "tabs.query"));
    assert.ok(!reached(chrome, "scripting.executeScript"));
  }
});

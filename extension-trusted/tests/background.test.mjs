// The service worker against a fake browser: it answers the BraveBot extension
// and no one else, takes a connection from its own two windows and nothing else,
// and sets the windows up as it starts.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { confirm } from "../approval.js";
import { FIRST_ID, SECOND_ID, isFirst } from "../pair.js";
import { Failure, Refusal, SENTENCES } from "../sentences.js";
import { askDialog } from "../windows.js";
import { callsOf, fakeBrowser, pageAt } from "./fake-debugger.mjs";
import { flush } from "./fake-panels.mjs";
import { scripts } from "./scripts.mjs";

// The reply when no tab is at the address of the request: the request reached
// its tool, which asked the browser for the tabs.
const NO_TAB = {
  v: 1,
  error: { kind: "failed", message: SENTENCES.tabs(0) },
};
const HOVER = {
  v: 1,
  method: "hover_element",
  params: { url: "https://example.test/page", role: "link", name: "Docs" },
};

// The worker loaded once for a fake browser, as the browser loads it.
let loads = 0;
async function load(fake) {
  const had = globalThis.chrome;
  globalThis.chrome = fake.chrome;
  try {
    loads += 1;
    await import(`../background.js?load=${loads}`);
  } finally {
    globalThis.chrome = had;
  }
  return fake;
}

test("the worker registers its listeners as it starts, at the top level: one for external messages, and one each for the connections and the removed windows of its own two windows", async () => {
  const fake = await load(fakeBrowser());
  assert.equal(fake.listeners.length, 1);
  assert.equal(fake.connections.length, 1);
  assert.equal(fake.removals.length, 1);
  // The browser it was given has these events and no other.
  assert.deepEqual(Object.keys(fake.chrome.runtime).sort(), [
    "getContexts",
    "getURL",
    "id",
    "onConnect",
    "onMessageExternal",
  ]);
  assert.deepEqual(Object.keys(fake.chrome.windows).sort(), ["create", "onRemoved", "remove"]);
  const source = readFileSync(new URL("../background.js", import.meta.url), "utf8");
  assert.deepEqual(
    source.split("\n").filter((line) => line.includes("addListener")),
    ["chrome.runtime.onMessageExternal.addListener((message, sender, sendResponse) => {"],
  );
});

// The lines of each script that register a listener on an event of the browser.
function registrationsBy(name) {
  const found = scripts().find((script) => script.name === name);
  return found.code
    .map((line) => line.trim())
    .filter((line) => /\bchrome\s*\.\s*\w+\s*\.\s*on\w+\s*\.\s*addListener/.test(line));
}

test("the scripts register a listener for an event of the browser in three files only: the external message in background.js, and the connection and the removed window in windows.js and in the shared confirmation", () => {
  const expected = {
    "background.js": ["chrome.runtime.onMessageExternal.addListener((message, sender, sendResponse) => {"],
    "windows.js": [
      "chrome.runtime.onConnect.addListener((port) => (connections.get(pageOf(port)) ?? sendAway)(port));",
      "chrome.windows.onRemoved.addListener((id) => {",
    ],
    "confirmation.js": [
      "chrome.runtime.onConnect.addListener(connected);",
      "chrome.windows.onRemoved.addListener(removed);",
    ],
  };
  const names = scripts().map(({ name }) => name);
  assert.ok(names.length >= 12, `${names.length} scripts were read`);
  for (const name of names) {
    assert.deepEqual(registrationsBy(name), expected[name] ?? [], name);
  }
  assert.ok(Object.keys(expected).every((name) => names.includes(name)));
  // The scan finds a listener written for another event.
  const sample = "chrome.runtime.onMessage.addListener(handle);";
  assert.ok(/\bchrome\s*\.\s*\w+\s*\.\s*on\w+\s*\.\s*addListener/.test(sample));
});

test("the worker sets its windows up as it starts: an action is asked about in the confirmation window, a dialog in the panel, and windows an earlier worker left are closed", async () => {
  const fake = await load(fakeBrowser());
  await flush();
  const asked = fake.windowCalls.filter(([call]) => call === "getContexts");
  assert.equal(asked.length, 2, "the windows of both pages are looked for");
  fake.contexts.push(
    { contextType: "TAB", documentUrl: "chrome-extension://fakesecondextension/dialog.html", windowId: 31 },
    { contextType: "TAB", documentUrl: "chrome-extension://fakesecondextension/popup.html", windowId: 32 },
  );
  const again = await load(fake);
  await flush();
  assert.deepEqual(
    again.windowCalls.filter(([call]) => call === "remove"),
    [["remove", 31]],
  );

  const question = confirm({ description: { action: "click" } });
  const settled = question.then(
    () => "allowed",
    (error) => error,
  );
  await flush();
  const created = fake.windowCalls.filter(([call]) => call === "create");
  assert.deepEqual(created.map(([, options]) => options.url), [
    "chrome-extension://fakesecondextension/confirm.html",
  ]);
  // The window is closed without an answer: a refusal of this extension's table.
  fake.removals.at(-1)(9);
  const error = await settled;
  assert.ok(error instanceof Refusal && error.code === "closed", String(error?.code));

  // A dialog asked about in the time left of the call that asked about an action
  // opens the panel, through the function the tools use.
  const call = { description: { action: "click" } };
  const next = confirm(call);
  next.catch(() => {});
  await flush();
  const panel = askDialog(
    { type: "alert", message: "m", defaultPrompt: "" },
    { origin: "https://example.test", url: "https://example.test/", first: call },
  );
  panel.catch(() => {});
  await flush();
  assert.deepEqual(
    fake.windowCalls.filter(([call2]) => call2 === "create").map(([, options]) => options.url).at(-1),
    "chrome-extension://fakesecondextension/dialog.html",
  );

  // A dialog with no action window before it has no time and asks nothing.
  const before = fake.windowCalls.length;
  await assert.rejects(
    askDialog(
      { type: "alert", message: "m", defaultPrompt: "" },
      { origin: "https://example.test", url: "https://example.test/", first: {} },
    ),
    (failure) => failure instanceof Failure && failure.code === "dialog_open",
  );
  assert.equal(fake.windowCalls.length, before);

  // The two windows still open are closed, so that no wait of 60 seconds is left
  // running when the test ends.
  for (const removed of fake.removals) {
    removed(9);
  }
  await flush();
});

test("a message from the BraveBot extension is answered, with the channel kept open for the reply, by the tool of its method against the browser the worker runs in", async () => {
  const fake = await load(fakeBrowser());
  await flush();
  const started = fake.calls.length;
  const had = globalThis.chrome;
  globalThis.chrome = fake.chrome;
  try {
    const answered = await fake.dispatch(HOVER, { id: FIRST_ID });
    assert.deepEqual(answered, { replied: true, response: NO_TAB });
    assert.deepEqual(
      fake.calls.slice(started).map((entry) => entry.call),
      ["getTargets"],
    );
    const refused = await fake.dispatch({ v: 1, method: "nothing" }, { id: FIRST_ID });
    assert.equal(refused.replied, true);
    assert.equal(refused.response.error.kind, "refusal");
  } finally {
    globalThis.chrome = had;
  }
});

test("a message from any other sender gets no reply of any content, and reaches nothing", async () => {
  const fake = await load(fakeBrowser());
  await flush();
  const started = fake.calls.length;
  for (const sender of [
    undefined,
    null,
    {},
    { id: SECOND_ID },
    { id: "abcdefghijklmnopabcdefghijklmnop" },
    { id: FIRST_ID.toUpperCase() },
    { id: `${FIRST_ID} ` },
    { id: ` ${FIRST_ID}` },
    { id: `${FIRST_ID}a` },
    { id: FIRST_ID.slice(1) },
    { id: [FIRST_ID] },
    { url: `chrome-extension://${FIRST_ID}/background.js` },
    { origin: `chrome-extension://${FIRST_ID}` },
    { url: "https://example.test/", tab: { id: 3 } },
    { id: undefined, url: `chrome-extension://${FIRST_ID}/` },
    // A script of the BraveBot extension that runs in a page has its id and a tab.
    { id: FIRST_ID, tab: { id: 3 } },
    { id: FIRST_ID, tab: { id: 3 }, frameId: 0, url: "https://example.test/" },
    { id: FIRST_ID, tab: { id: 3 }, frameId: 2, url: `chrome-extension://${FIRST_ID}/background.js` },
    FIRST_ID,
  ]) {
    const answered = await fake.dispatch(HOVER, sender);
    assert.deepEqual(answered, { replied: false }, JSON.stringify(sender));
  }
  assert.deepEqual(fake.calls.slice(started), []);
});

test("only the id of the BraveBot extension is the first", () => {
  assert.equal(isFirst({ id: FIRST_ID }), true);
  assert.equal(isFirst({ id: SECOND_ID }), false);
  assert.equal(isFirst({ id: FIRST_ID, tab: undefined }), true);
  // The worker of the BraveBot extension has no tab. A script of it that runs in a
  // page has one, and is not answered, whatever its frame and address say.
  assert.equal(isFirst({ id: FIRST_ID, origin: `chrome-extension://${FIRST_ID}`, url: `chrome-extension://${FIRST_ID}/background.js` }), true);
  assert.equal(isFirst({ id: FIRST_ID, tab: { id: 3 } }), false);
  assert.equal(isFirst({ id: FIRST_ID, tab: null }), false);
  assert.equal(isFirst({ id: FIRST_ID, tab: { id: 3 }, frameId: 0, url: `chrome-extension://${FIRST_ID}/` }), false);
  assert.equal(isFirst(undefined), false);
  assert.equal(isFirst(null), false);
  assert.equal(isFirst(FIRST_ID), false);
});

// The targets the browser lists: the tab of the page, which a worker that stopped
// left attached, a second page that is not attached, a third that another
// debugger holds and whose detach fails, and a worker that is not a page.
function leftOver() {
  const fake = fakeBrowser({
    targets: [
      { ...pageAt("https://example.test/page", 7), attached: true },
      pageAt("https://example.test/other", 8),
      { ...pageAt("https://example.test/devtools", 9), attached: true },
      { type: "service_worker", id: "worker", url: "chrome-extension://x/sw.js", attached: true },
    ],
  });
  fake.attached.add(7);
  const api = fake.chrome.debugger;
  const detach = api.detach;
  api.detach = async (target) => {
    if (target.tabId === 9) {
      fake.calls.push({ call: "detach", tabId: 9 });
      throw new Error("Debugger is not attached to the tab with id: 9.");
    }
    return detach(target);
  };
  return fake;
}

test("as the worker starts it detaches the debugger from every tab it is still attached to, which a worker that stopped left, and from no other target, and a tab another debugger holds changes nothing", async () => {
  const fake = await load(leftOver());
  await flush();
  assert.deepEqual(
    callsOf(fake, "detach").map((entry) => entry.tabId),
    [7, 9],
    "the tabs a debugger is attached to",
  );
  assert.equal(fake.attached.has(7), false, "the session of the worker that stopped is still there");
  assert.deepEqual(callsOf(fake, "attach"), []);
  // Nothing about the page is read or sent.
  assert.deepEqual(callsOf(fake, "sendCommand"), []);
  // A browser that cannot list its tabs leaves the worker running.
  const broken = fakeBrowser();
  broken.chrome.debugger.getTargets = () => Promise.reject(new Error("gone"));
  await load(broken);
  await flush();
  assert.deepEqual(callsOf(broken, "detach"), []);
});

test("a call that comes with the start of the worker attaches only after the leftover sessions are detached, so it is not detached by the clean-up", async () => {
  const fake = leftOver();
  const api = fake.chrome.debugger;
  const listing = api.getTargets;
  let release;
  let first = true;
  api.getTargets = () => {
    if (first) {
      first = false;
      return new Promise((resolve) => {
        release = () => resolve(listing());
      });
    }
    return listing();
  };
  await load(fake);
  const had = globalThis.chrome;
  globalThis.chrome = fake.chrome;
  let answered;
  try {
    const reply = fake.dispatch(HOVER, { id: FIRST_ID });
    await flush();
    assert.deepEqual(callsOf(fake, "attach"), [], "the call attached while the clean-up was listing the tabs");
    release();
    answered = await reply;
  } finally {
    globalThis.chrome = had;
  }
  assert.equal(answered.replied, true);
  const order = fake.calls.map((entry) => entry.call);
  assert.ok(order.indexOf("detach") < order.indexOf("attach"), order.join());
  assert.equal(callsOf(fake, "attach").length, 1, "the call did not attach");
  assert.equal(fake.attached.size, 0, "a session is left attached");
});

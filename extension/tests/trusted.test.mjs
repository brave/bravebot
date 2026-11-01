// The tools of the input-trusted group, which this extension checks and sends to
// the BraveBot Trusted Input extension: what has to hold before anything is sent,
// what is sent, and what each reply or failure of the other extension becomes.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { ACTIVITY_KEY, activityLog } from "../activity.js";
import { NO_ACCESS } from "../groups.js";
import { RUNS_KEY } from "../lifetimes.js";
import { setTool, turnOn } from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  BUSY_MESSAGE,
  GROUP_SWITCHES_KEY,
  SITE_SCOPES_KEY,
  TOOLS,
  handle,
} from "../tools.js";
import {
  NOT_THERE,
  NOT_UNDERSTOOD,
  SECOND_ID,
  SENTENCE_LIMIT,
  STATUSES,
  PICTURE_METHODS,
  STOPPED,
  TOO_LATE,
  VERSION,
  WAIT_MS,
  forward,
} from "../trusted.js";
import { FIRST_ID, SECOND_ID as SECONDS_OWN_ID } from "../../extension-trusted/pair.js";
import {
  METHODS,
  PICTURES as SECONDS_PICTURES,
  STATUSES as SECONDS_STATUSES,
  VERSION as SECONDS_VERSION,
  answer,
  parse,
} from "../../extension-trusted/requests.js";
import { Failure, Refusal } from "../../extension-trusted/sentences.js";
import { fakeChrome } from "./fake-chrome.mjs";

afterEach(() => mock.timers.reset());

const NEWS = "https://news.example";
const PAGE = `${NEWS}/front`;
const BANK = "https://bank.example";

// The arguments of a good call of each tool, at the page above.
const CALLS = {
  click_element_trusted: { url: PAGE, role: "button", name: "Save" },
  type_text_trusted: { url: PAGE, role: "textbox", name: "Name", text: "Ada" },
  press_key_trusted: { url: PAGE, key: "Enter" },
  hover_element: { url: PAGE, role: "link", name: "Docs" },
  drag_element: {
    url: PAGE,
    role: "option",
    name: "One",
    onto_role: "option",
    onto_name: "Two",
  },
};
const TRUSTED = Object.keys(CALLS);

const DONE = { v: VERSION, result: { status: "clicked", origin: NEWS } };

// A browser in which the person turned input-trusted on, allowed the sites in
// `allowed` and blocked those in `blocked`, and in which the other extension
// answers each message with `reply`: a reply, or a function of the id sent to and
// the request that gives one or throws. `sent` lists every message that was sent.
// The group is on until Brave restarts, as its default is.
async function trusted({
  allowed = [NEWS],
  blocked = [],
  reply = DONE,
  local = {},
} = {}) {
  const chrome = fakeChrome({
    held: {},
    local: {
      [ALLOWED_SITES_KEY]: allowed,
      [BLOCKED_SITES_KEY]: blocked,
      ...local,
    },
    tabs: [{ id: 1, windowId: 1, title: "News", url: PAGE }],
  });
  chrome.sent = [];
  chrome.runtime.sendMessage = async (id, request) => {
    chrome.sent.push([id, request]);
    return typeof reply === "function" ? reply(id, request) : reply;
  };
  assert.equal(await turnOn(chrome, "input-trusted", NO_ACCESS), true);
  return chrome;
}

const call = (chrome, method, params) =>
  handle({ id: 7, method, params }, chrome);

// Lets what is waiting on a resolved promise run.
async function settle() {
  for (let turn = 0; turn < 50; turn += 1) {
    await Promise.resolve();
  }
}

// ---- the pair of extensions ----

// The ids, the version, the status words and the methods are written in both
// extensions, and a difference between them makes every call fail or be
// refused by the other.
test("this extension sends to the id the other extension has, in the version and with the methods and status words it knows", () => {
  assert.equal(SECOND_ID, SECONDS_OWN_ID);
  assert.notEqual(SECOND_ID, FIRST_ID);
  assert.equal(VERSION, SECONDS_VERSION);
  assert.deepEqual([...STATUSES], [...SECONDS_STATUSES]);
  // The methods the other extension answers are these five, the one whose result
  // is a picture, the one whose result is a console and the one that chooses a file.
  assert.deepEqual([...TRUSTED, ...PICTURE_METHODS, "read_console", "choose_file"], Object.keys(METHODS));
  assert.deepEqual(
    [...TRUSTED, ...PICTURE_METHODS, "read_console", "choose_file"],
    Object.keys(TOOLS).filter((tool) => Object.hasOwn(METHODS, tool)),
  );
  assert.deepEqual([...PICTURE_METHODS], [...SECONDS_PICTURES]);
});

// What the other extension's request check takes, a request of each tool is
// accepted by it with exactly the arguments the call gave and the defaults
// filled in, so a tool cannot send an argument the other refuses or one it
// never takes.
test("each tool sends a request the other extension's check accepts, with the arguments the call gave and no other", async () => {
  const chrome = await trusted({
    reply: (id, request) => answer(request, "the browser", handlersOf(request)),
  });
  for (const method of TRUSTED) {
    chrome.sent.length = 0;
    const reply = await call(chrome, method, {
      ...CALLS[method],
      selector: "#save",
      x: 10,
      y: 20,
      tab_id: 1,
      coordinates: [1, 2],
    });
    assert.equal(reply.error, undefined, `${method}: ${reply.error?.message}`);
    assert.equal(chrome.sent.length, 1, method);
    const [id, request] = chrome.sent[0];
    assert.equal(id, SECOND_ID);
    assert.equal(request.v, VERSION);
    assert.equal(request.method, method);
    const { method: parsedMethod, args } = parse(request);
    assert.equal(parsedMethod, method);
    const given = Object.keys(CALLS[method]).sort();
    const sent = Object.keys(request.params).sort();
    const defaults = method === "click_element_trusted" ? ["button", "count"] : [];
    assert.deepEqual(sent, [...given, ...defaults].sort(), method);
    for (const name of given) {
      assert.equal(request.params[name], CALLS[method][name], `${method} ${name}`);
    }
    assert.deepEqual({ ...args }, request.params, method);
  }
});

// The handlers of the other extension's request check, standing in for the
// tools, which answer that the method was done.
function handlersOf(request) {
  return {
    [request.method]: async () => ({ status: "clicked", origin: NEWS }),
  };
}

test("a click's button and count are sent as given, and the left button once where the call gives none", async () => {
  const chrome = await trusted();
  await call(chrome, "click_element_trusted", CALLS.click_element_trusted);
  assert.deepEqual(chrome.sent.at(-1)[1].params, {
    ...CALLS.click_element_trusted,
    button: "left",
    count: 1,
  });
  await call(chrome, "click_element_trusted", {
    ...CALLS.click_element_trusted,
    button: "left",
    count: 2,
  });
  assert.equal(chrome.sent.at(-1)[1].params.count, 2);
  await call(chrome, "click_element_trusted", {
    ...CALLS.click_element_trusted,
    button: "right",
  });
  assert.equal(chrome.sent.at(-1)[1].params.button, "right");
  assert.equal(chrome.sent.length, 3);
});

// ---- what has to hold before anything is sent ----

// The checks come first, in this order. A forward that came before any of them
// would send a request for a group that is off, a tool that is off, a group
// whose hour has ended, a site nobody allowed or a site that is blocked.
test("nothing is sent while the group is off, the tool is off, the lifetime has ended, the site is not allowed or the site is blocked", async () => {
  for (const method of TRUSTED) {
    const params = CALLS[method];

    const off = await trusted();
    off.local[GROUP_SWITCHES_KEY] = {};
    const groupOff = await call(off, method, params);
    assert.match(groupOff.error.message, /in the input-trusted group, which is turned off/, method);
    assert.deepEqual(off.sent, [], `${method} with the group off`);

    const toolOff = await trusted();
    await setTool(toolOff, method, false);
    const toolReply = await call(toolOff, method, params);
    assert.match(toolReply.error.message, /is turned off in the BraveBot extension's options/, method);
    assert.deepEqual(toolOff.sent, [], `${method} with the tool off`);

    const lapsed = await trusted();
    lapsed.session[RUNS_KEY] = {};
    const lapsedReply = await call(lapsed, method, params);
    assert.match(lapsedReply.error.message, /was turned on until Brave restarts, and that has ended/, method);
    assert.deepEqual(lapsed.sent, [], `${method} with the lifetime ended`);

    const elsewhere = await trusted({ allowed: [BANK] });
    const notAllowed = await call(elsewhere, method, params);
    assert.match(notAllowed.error.message, /not in the allowed sites list/, method);
    assert.deepEqual(elsewhere.sent, [], `${method} at a site that is not allowed`);

    const blocked = await trusted({ allowed: [NEWS], blocked: [NEWS] });
    const blockedReply = await call(blocked, method, params);
    assert.match(blockedReply.error.message, /in the blocked sites list/, method);
    assert.deepEqual(blocked.sent, [], `${method} at a blocked site`);
  }
});

// The group has no scope. A stored choice of all sites for it, which the options
// page never writes for this group, does not let a call reach a site nobody
// allowed, and a list of allowed sites that cannot be read allows none.
test("the group is limited to the allowed sites whatever scope is stored, and an allowed list that cannot be read allows none", async () => {
  for (const method of TRUSTED) {
    const params = CALLS[method];
    const widened = await trusted({
      allowed: [BANK],
      local: { [SITE_SCOPES_KEY]: { "input-trusted": "all", input: "all" } },
    });
    const reply = await call(widened, method, params);
    assert.match(reply.error.message, /not in the allowed sites list/, method);
    assert.deepEqual(widened.sent, [], method);

    const unreadable = await trusted({ allowed: "https://news.example" });
    const refused = await call(unreadable, method, params);
    assert.match(refused.error.message, /not in the allowed sites list/, method);
    assert.deepEqual(unreadable.sent, [], method);

    const allowed = await trusted();
    const ran = await call(allowed, method, params);
    assert.equal(ran.error, undefined, method);
    assert.equal(allowed.sent.length, 1, method);
  }
});

test("a page that is not on the web is not sent, even where its origin is an allowed one", async () => {
  for (const url of [
    `blob:${NEWS}/0b1c2d3e`,
    `view-source:${PAGE}`,
    `about:blank`,
  ]) {
    const chrome = await trusted({ allowed: [NEWS, "about://blank"] });
    const reply = await call(chrome, "hover_element", { ...CALLS.hover_element, url });
    assert.ok(reply.error, url);
    assert.deepEqual(chrome.sent, [], url);
  }
});

test("an argument that is not on a closed list or is out of its bounds is refused before anything is sent", async () => {
  const bad = [
    ["click_element_trusted", { role: "div" }],
    ["click_element_trusted", { name: "" }],
    ["click_element_trusted", { name: "x".repeat(201) }],
    ["click_element_trusted", { button: "middle" }],
    ["click_element_trusted", { count: 3 }],
    ["click_element_trusted", { count: 0 }],
    ["click_element_trusted", { button: "right", count: 2 }],
    ["type_text_trusted", { text: "" }],
    ["type_text_trusted", { text: "x".repeat(2001) }],
    ["type_text_trusted", { role: "paragraph" }],
    ["press_key_trusted", { key: "a" }],
    ["press_key_trusted", { key: "Control+a" }],
    ["hover_element", { role: "" }],
    ["drag_element", { onto_role: "div" }],
    ["drag_element", { onto_name: "" }],
    ["drag_element", { onto_name: "x".repeat(201) }],
    ["drag_element", { onto_role: undefined }],
    ["drag_element", { onto_name: undefined }],
    ["click_element_trusted", { url: undefined }],
    ["click_element_trusted", { url: 5 }],
  ];
  for (const [method, change] of bad) {
    const chrome = await trusted();
    const params = { ...CALLS[method], ...change };
    const reply = await call(chrome, method, params);
    assert.ok(reply.error, `${method} ${JSON.stringify(change)}`);
    assert.deepEqual(chrome.sent, [], `${method} ${JSON.stringify(change)}`);
  }
});

// ---- what a reply or a failure becomes ----

test("a reply of a status word and an origin is the result, and nothing else the reply holds is passed on", async () => {
  for (const status of STATUSES) {
    const chrome = await trusted({
      reply: {
        v: VERSION,
        result: { status, origin: NEWS, title: "Page", text: "secret", extra: 1 },
        note: "also this",
      },
    });
    const reply = await call(chrome, "hover_element", CALLS.hover_element);
    assert.deepEqual(reply.result, { status, origin: NEWS }, status);
  }
});

test("a refusal of the other extension is a refusal here and a failure of it is a failure, each with its sentence after fixed words", async () => {
  const sentence = "it is a password field, which is never acted on";
  const refused = await trusted({
    reply: { v: VERSION, error: { kind: "refusal", message: sentence } },
  });
  const refusal = await call(refused, "type_text_trusted", CALLS.type_text_trusted);
  assert.equal(
    refusal.error.message,
    `the BraveBot Trusted Input extension refused it: ${sentence}`,
  );
  const failed = await trusted({
    reply: { v: VERSION, error: { kind: "failed", message: "the request could not be completed" } },
  });
  const failure = await call(failed, "type_text_trusted", CALLS.type_text_trusted);
  assert.equal(
    failure.error.message,
    "the BraveBot Trusted Input extension could not do it: the request could not be completed",
  );
  const log = await activityLog(refused);
  assert.equal(log.at(-1).outcome, "refused");
  assert.equal((await activityLog(failed)).at(-1).outcome, "failed");
});

// Whatever the other extension answers that is not a status word with an origin
// or a kind word with one plain sentence is a failure with a sentence written
// here, which holds none of the reply. A stranger that took the id, or a bug in
// the other extension, therefore cannot put its own text in a reply or turn a
// failure into a result.
test("a reply that is not made of words this extension knows is a failure with a fixed sentence", async () => {
  const HIDDEN = "‮evil‬";
  const bad = [
    undefined,
    null,
    "clicked",
    42,
    [],
    {},
    { v: 2, result: { status: "clicked", origin: NEWS } },
    { result: { status: "clicked", origin: NEWS } },
    { v: VERSION },
    { v: VERSION, result: {} },
    { v: VERSION, result: "clicked" },
    { v: VERSION, result: { status: "launched the missiles", origin: NEWS } },
    { v: VERSION, result: { status: "clicked" } },
    { v: VERSION, result: { status: "clicked", origin: "not an origin" } },
    { v: VERSION, result: { status: "clicked", origin: `${NEWS}/path` } },
    { v: VERSION, result: { status: "clicked", origin: "javascript:alert(1)" } },
    { v: VERSION, result: { status: "clicked", origin: 5 } },
    { v: VERSION, result: { status: "clicked", origin: NEWS }, error: { kind: "failed", message: "x" } },
    { v: VERSION, error: "it failed" },
    { v: VERSION, error: { message: "it failed" } },
    { v: VERSION, error: { kind: "exploded", message: "it failed" } },
    { v: VERSION, error: { kind: "refusal" } },
    { v: VERSION, error: { kind: "refusal", message: "" } },
    { v: VERSION, error: { kind: "refusal", message: 5 } },
    { v: VERSION, error: { kind: "refusal", message: `fine ${HIDDEN}` } },
    { v: VERSION, error: { kind: "failed", message: "line one\nline two" } },
    { v: VERSION, error: { kind: "failed", message: "x".repeat(SENTENCE_LIMIT + 1) } },
    { v: VERSION, error: { kind: "failed", message: "a\u0000b" } },
  ];
  for (const reply of bad) {
    const chrome = await trusted({ reply: () => reply });
    const answered = await call(chrome, "click_element_trusted", CALLS.click_element_trusted);
    assert.equal(answered.result, undefined, JSON.stringify(reply));
    assert.equal(answered.error.message, NOT_UNDERSTOOD, JSON.stringify(reply));
    assert.equal(
      (await activityLog(chrome)).at(-1).outcome,
      "failed",
      JSON.stringify(reply),
    );
  }
  // The longest plain sentence is passed on.
  const edge = "x".repeat(SENTENCE_LIMIT);
  const chrome = await trusted({
    reply: { v: VERSION, error: { kind: "failed", message: edge } },
  });
  const answered = await call(chrome, "hover_element", CALLS.hover_element);
  assert.match(answered.error.message, new RegExp(`${edge}$`));
});

test("an extension that is not installed, is turned off or does not accept this one is one sentence of refusal", async () => {
  const absent = new Error("Could not establish connection. Receiving end does not exist.");
  for (const method of TRUSTED) {
    const chrome = await trusted({
      reply: () => {
        throw absent;
      },
    });
    const reply = await call(chrome, method, CALLS[method]);
    assert.equal(reply.error.message, NOT_THERE, method);
    assert.equal((await activityLog(chrome)).at(-1).outcome, "refused", method);
  }
  // A sentence on its own, with no word of the browser's.
  assert.doesNotMatch(NOT_THERE, /Receiving end|connection/);
  assert.equal(NOT_THERE.split(". ").length, 1);
});

// Absent is a refusal. A worker that stopped after the request was sent may have
// acted, which a refusal would not say, so it is a failure that says so.
test("a worker that stops before it answers is a failure that says the action may have been done", async () => {
  const chrome = await trusted({
    reply: () => {
      throw new Error("The message port closed before a response was received.");
    },
  });
  const reply = await call(chrome, "click_element_trusted", CALLS.click_element_trusted);
  assert.equal(reply.error.message, STOPPED);
  assert.match(STOPPED, /may or may not have been done/);
  assert.equal((await activityLog(chrome)).at(-1).outcome, "failed");
});

// A send that never ends is ended here, so the relay (90 seconds) is answered
// by this extension and does not time out on its own, and the person's 60 seconds
// in the other extension's window have gone by.
test("a reply that does not come in time ends the call at the wait and not before", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  const chrome = await trusted();
  chrome.runtime.sendMessage = () => new Promise(() => {});
  let outcome;
  forward(chrome, "hover_element", CALLS.hover_element).then(
    () => (outcome = "done"),
    (error) => (outcome = error),
  );
  await settle();
  mock.timers.tick(WAIT_MS - 1);
  await settle();
  assert.equal(outcome, undefined);
  mock.timers.tick(1);
  await settle();
  assert.ok(outcome, "the call was not ended at the wait");
  assert.equal(outcome.message, TOO_LATE);
  assert.equal(outcome.constructor.name, "ToolError");
  assert.match(TOO_LATE, /80 seconds/);
  assert.equal(WAIT_MS, 80_000);
});

test("a reply in time leaves no timer behind", async () => {
  const before = process.getActiveResourcesInfo().filter((name) => name === "Timeout").length;
  const chrome = await trusted();
  await forward(chrome, "hover_element", CALLS.hover_element);
  const failing = await trusted({
    reply: () => {
      throw new Error("Receiving end does not exist");
    },
  });
  await assert.rejects(forward(failing, "hover_element", CALLS.hover_element));
  const after = process.getActiveResourcesInfo().filter((name) => name === "Timeout").length;
  assert.equal(after, before);
});

// ---- one at a time, across both extensions ----

test("while a call waits for the other extension no other action is started, and the next is accepted once it ends, however it ended", async () => {
  const chrome = await trusted();
  let release;
  chrome.runtime.sendMessage = (id, request) => {
    chrome.sent.push([id, request]);
    return new Promise((resolve) => {
      release = resolve;
    });
  };
  // An action of the input group, which has its own tools and asks in this
  // extension's own window, takes the same place in line.
  assert.equal(
    await turnOn(chrome, "input", { allowed: [NEWS], scopes: {} }),
    true,
  );
  const first = call(chrome, "click_element_trusted", CALLS.click_element_trusted);
  await settle();
  assert.equal(chrome.sent.length, 1);
  const second = await call(chrome, "hover_element", CALLS.hover_element);
  assert.equal(second.error.message, BUSY_MESSAGE);
  assert.equal(chrome.sent.length, 1, "a second request was sent");
  assert.equal((await activityLog(chrome)).at(-1).outcome, "refused");
  const inPage = await call(chrome, "click_element", CALLS.click_element_trusted);
  assert.equal(inPage.error.message, BUSY_MESSAGE);
  assert.equal(chrome.sent.length, 1, "an action of input sent a request");
  release(DONE);
  assert.equal((await first).result.status, "clicked");

  // A failure releases the place as a result does.
  chrome.runtime.sendMessage = async () => {
    throw new Error("The message port closed before a response was received.");
  };
  const failed = await call(chrome, "hover_element", CALLS.hover_element);
  assert.equal(failed.error.message, STOPPED);
  chrome.runtime.sendMessage = async (id, request) => {
    chrome.sent.push([id, request]);
    return DONE;
  };
  const again = await call(chrome, "hover_element", CALLS.hover_element);
  assert.equal(again.result.status, "clicked");
  // And so does a call the checks refuse before it waits.
  const refused = await call(chrome, "hover_element", { ...CALLS.hover_element, role: "div" });
  assert.ok(refused.error);
  const last = await call(chrome, "hover_element", CALLS.hover_element);
  assert.equal(last.result.status, "clicked");
});

// ---- the other groups ----

// An extension that is not there fails the five tools of its group and leaves
// every other group as it was: their switches and permissions are not touched,
// and their tools answer.
test("the other extension being absent changes nothing in any other group", async () => {
  const chrome = await trusted({
    reply: () => {
      throw new Error("Could not establish connection. Receiving end does not exist.");
    },
  });
  assert.equal(await turnOn(chrome, "tabs-read", NO_ACCESS), true);
  const switches = structuredClone(chrome.local[GROUP_SWITCHES_KEY]);
  const settings = structuredClone(chrome.local.tools);
  const before = chrome.calls.length;
  const refused = await call(chrome, "drag_element", CALLS.drag_element);
  assert.equal(refused.error.message, NOT_THERE);
  assert.deepEqual(chrome.local[GROUP_SWITCHES_KEY], switches);
  assert.deepEqual(chrome.local.tools, settings);
  assert.deepEqual(chrome.calls.slice(before), []);
  assert.equal((await call(chrome, "list_tabs")).result.length, 1);
  assert.equal((await call(chrome, "get_platform_info")).result.os, "mac");
  assert.equal((await call(chrome, "hover_element", CALLS.hover_element)).error.message, NOT_THERE);
});

// ---- the activity log ----

test("the activity log keeps the group, the tool, the outcome and the origin of the page and nothing the call wrote", async () => {
  const chrome = await trusted();
  const secret = {
    ...CALLS.type_text_trusted,
    name: "Card number field",
    text: "4111 1111 1111 1111",
    url: `${PAGE}?token=abc#frag`,
  };
  const reply = await call(chrome, "type_text_trusted", secret);
  assert.equal(reply.result.status, "clicked");
  await call(chrome, "drag_element", { ...CALLS.drag_element, onto_name: "Trash" });
  const entries = await activityLog(chrome);
  assert.equal(entries.length, 2);
  assert.deepEqual(
    entries.map(({ group, tool, outcome, origins }) => ({ group, tool, outcome, origins })),
    [
      { group: "input-trusted", tool: "type_text_trusted", outcome: "ran", origins: [NEWS] },
      { group: "input-trusted", tool: "drag_element", outcome: "ran", origins: [NEWS] },
    ],
  );
  const stored = JSON.stringify(chrome.local[ACTIVITY_KEY]);
  for (const word of ["4111", "Card number", "Trash", "token", "frag", "front", "textbox", "Ada"]) {
    assert.ok(!stored.includes(word), `the log holds ${word}`);
  }
  assert.deepEqual(Object.keys(entries[0]).sort(), ["group", "origins", "outcome", "time", "tool"]);
});

test("a call that is refused is logged as refused with the origin, and the group's tools are logged though the group is off", async () => {
  const chrome = await trusted({ allowed: [BANK] });
  await call(chrome, "click_element_trusted", CALLS.click_element_trusted);
  chrome.local[GROUP_SWITCHES_KEY] = {};
  await call(chrome, "hover_element", CALLS.hover_element);
  const entries = await activityLog(chrome);
  assert.deepEqual(
    entries.map(({ tool, outcome, origins }) => [tool, outcome, origins]),
    [
      ["click_element_trusted", "refused", [NEWS]],
      ["hover_element", "refused", [NEWS]],
    ],
  );
  assert.deepEqual(chrome.sent, []);
});

// ---- this extension only sends ----

const DIRECTORY = new URL("../", import.meta.url);

const scripts = () =>
  readdirSync(DIRECTORY)
    .filter((name) => name.endsWith(".js"))
    .map((name) => [name, readFileSync(new URL(name, DIRECTORY), "utf8")]);

test("only trusted.js sends a message to another extension, and no script listens for one", () => {
  const code = (text) =>
    text.split("\n").filter((line) => !line.trim().startsWith("//"));
  // A message sent with no extension id, as the capture window's is, reaches
  // this extension's own worker.
  const toItself = (line) => /\bsendMessage\(\{/.test(line);
  const sending = scripts()
    .filter(([, text]) =>
      code(text).some((line) => /\bsendMessage\b/.test(line) && !toItself(line)),
    )
    .map(([name]) => name);
  assert.deepEqual(sending, ["trusted.js"]);
  const sendingToItself = scripts()
    .filter(([, text]) => code(text).some(toItself))
    .map(([name]) => name);
  assert.deepEqual(sendingToItself, ["capture.js"]);
  for (const [name, text] of scripts()) {
    for (const line of code(text)) {
      assert.doesNotMatch(
        line,
        /onMessageExternal|onConnectExternal|onUserScriptMessage/,
        name,
      );
    }
  }
  const manifest = JSON.parse(readFileSync(new URL("manifest.json", DIRECTORY), "utf8"));
  assert.equal(manifest.externally_connectable, undefined);
  const trustedSends = code(readFileSync(new URL("trusted.js", DIRECTORY), "utf8")).filter(
    (line) => /\bsendMessage\b/.test(line),
  );
  assert.equal(trustedSends.length, 1);
  assert.match(trustedSends[0], /sendMessage\(SECOND_ID,/);
});

// The failures of the other extension's own reply code are the two classes this
// file maps, and its tools throw nothing else into a reply.
test("the other extension's refusals and failures arrive as the two kinds this extension maps", async () => {
  const refusing = await trusted({
    reply: (id, request) =>
      answer(request, "the browser", {
        [request.method]: async () => {
          throw new Refusal("password");
        },
      }),
  });
  const refused = await call(refusing, "type_text_trusted", CALLS.type_text_trusted);
  assert.equal(
    refused.error.message,
    "the BraveBot Trusted Input extension refused it: it is a password field, which is never acted on",
  );
  const failing = await trusted({
    reply: (id, request) =>
      answer(request, "the browser", {
        [request.method]: async () => {
          throw new Failure("covered");
        },
      }),
  });
  const failed = await call(failing, "click_element_trusted", CALLS.click_element_trusted);
  assert.equal(
    failed.error.message,
    "the BraveBot Trusted Input extension could not do it: another element covers the middle of it",
  );
  assert.equal((await activityLog(refusing)).at(-1).outcome, "refused");
  assert.equal((await activityLog(failing)).at(-1).outcome, "failed");
});

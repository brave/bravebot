// The tool read_console against a fake debugger that records every call, in
// order, and reports the entries of a console as the browser's Console and Log
// domains do, and a stand-in for the window the person answers in. The tests ask
// through requests.js, as the BraveBot extension does, so what they compare is the
// reply and what the browser was sent.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { approve, setConfirmation } from "../approval.js";
import { CONSOLE_SETTLE_MS, command, readConsole, withSession } from "../cdp.js";
import {
  DEFAULT_ENTRIES,
  LEVELS,
  LEVEL_WORDS,
  MAX_ENTRIES,
  READ_WAIT_MS,
  SOURCES,
  TEXT_LIMIT,
  URL_LIMIT,
  entryOf,
  isReport,
  limitsText,
} from "../consoles.js";
import { METHODS, REPORTS, answer } from "../requests.js";
import { Refusal, SENTENCES } from "../sentences.js";
import { callsOf, fakeDebugger, pageAt } from "./fake-debugger.mjs";
import { occurrences, scripts } from "./scripts.mjs";

afterEach(() => {
  setConfirmation();
  mock.timers.reset();
});

const URL_A = "https://example.test/page";
const ORIGIN = "https://example.test";
const METHOD = "read_console";
const TAB = 7;

// A request of the tool for the page.
const request = (params = {}) => ({ v: 1, method: METHOD, params: { url: URL_A, ...params } });

const refused = (code, detail) => ({ v: 1, error: { kind: "refusal", message: SENTENCES[code](detail) } });
const failed = (code, detail) => ({ v: 1, error: { kind: "failed", message: SENTENCES[code](detail) } });

// What the browser's Console domain reports for a call of console, and what its
// Log domain reports.
const said = (level, text, extra = {}) => ({
  message: { source: "console-api", level, text, url: `${URL_A}`, line: 3, column: 9, ...extra },
});
const logged = (level, text, extra = {}) => ({
  entry: { source: "network", level, text, timestamp: 1, url: `${ORIGIN}/missing.png`, networkRequestId: "1.2", ...extra },
});

// A page, a debugger over it and a stand-in for the window. `console` and `log` are
// the events the browser sends as each domain is enabled: a list of [method,
// params] pairs, or a function that returns one. `allow` is true to allow, false
// to answer nothing, or a function from the resolution to what the window answers.
function rig({ console: told = [], log: logs = [], allow = true, frame = URL_A, failOn, answers = {} } = {}) {
  let fake;
  const windows = [];
  const send = (events) => () => {
    for (const [method, params] of events) {
      fake.emit(method, params, TAB);
    }
    return {};
  };
  fake = fakeDebugger({
    targets: [pageAt(URL_A, TAB)],
    answers: {
      "Console.enable": send(told),
      "Log.enable": send(logs),
      "Page.getFrameTree": () => ({ frameTree: { frame: { url: frame } } }),
      ...answers,
    },
    failOn,
  });
  setConfirmation(async (resolution) => {
    windows.push({
      description: resolution.description,
      calls: fake.calls.map((entry) => entry.method ?? entry.call),
    });
    return typeof allow === "function" ? allow(resolution) : allow ? approve(resolution) : null;
  });
  return { fake, windows };
}

// Lets every promise that can settle without a timer settle.
const settle = () => new Promise((resolve) => setImmediate(resolve));

// The reply to a request, with the short wait for the browser's report passed.
async function run(r, params) {
  mock.timers.reset();
  mock.timers.enable({ apis: ["setTimeout"] });
  const reply = answer(request(params), r.fake.chrome);
  await settle();
  mock.timers.tick(CONSOLE_SETTLE_MS);
  return reply;
}

const order = (fake) => fake.calls.map((entry) => entry.method ?? entry.call);
const detaches = (fake) => callsOf(fake, "detach").length;
const entriesOf = (reply) => reply.result.entries;

test("the person is asked first, with the page's origin and address and how much is read and no element, before the debugger is attached or the console is asked for", async () => {
  const r = rig({ console: [["Console.messageAdded", said("log", "hello")]] });
  const reply = await run(r, { levels: "errors", max_entries: 20 });
  assert.equal(r.windows.length, 1);
  assert.deepEqual(r.windows[0].description, {
    action: "console",
    origin: ORIGIN,
    url: URL_A,
    tag: "",
    role: null,
    name: "",
    excerpt: "",
    goes: "",
    limits: "errors, the newest 20 at most",
  });
  assert.deepEqual(r.windows[0].calls, ["getTargets"], "the debugger was attached, or the console asked for, before the person answered");
  assert.deepEqual(order(r.fake), [
    "getTargets",
    "getTargets",
    "attach",
    "Page.enable",
    "Console.enable",
    "Log.enable",
    "Page.getFrameTree",
    "detach",
  ]);
  assert.equal(reply.v, 1);
});

test("a denial, a window that answers nothing and a token made for another question attach to nothing, enable no domain and read nothing", async () => {
  const cases = {
    "the person denies": { allow: () => Promise.reject(new Refusal("denied")), reply: refused("denied"), attaches: 0 },
    "the window answers nothing": { allow: false, reply: refused("denied"), attaches: 0 },
    "the window answers something that is not a token": { allow: () => "yes", reply: refused("denied"), attaches: 0 },
    "the token was made for another resolution": {
      allow: () => approve(Object.freeze({ description: {} })),
      reply: refused("not_allowed"),
      attaches: 1,
    },
  };
  for (const [label, { allow, reply, attaches }] of Object.entries(cases)) {
    const r = rig({ allow, console: [["Console.messageAdded", said("error", "secret")]] });
    assert.deepEqual(await run(r), reply, label);
    assert.equal(callsOf(r.fake, "attach").length, attaches, label);
    assert.equal(detaches(r.fake), attaches, label);
    const sent = order(r.fake);
    assert.ok(!sent.includes("Console.enable") && !sent.includes("Log.enable"), label);
  }
});

test("the entries of both domains are returned in the order the browser reported them, each with its level, source, text, script address and line and nothing else the browser sent", async () => {
  const r = rig({
    console: [
      ["Console.messageAdded", said("log", "log-line", { stackTrace: { callFrames: [] }, args: ["x"] })],
      ["Console.messageAdded", said("warning", "warn-line", { line: 9 })],
    ],
    log: [["Log.entryAdded", logged("error", "Failed to load resource: 404")]],
  });
  const reply = await run(r);
  assert.deepEqual(reply, {
    v: 1,
    result: {
      origin: ORIGIN,
      omitted: 0,
      entries: [
        { level: "log", source: "console-api", text: "log-line", cut: false, url: URL_A, line: 3 },
        { level: "warning", source: "console-api", text: "warn-line", cut: false, url: URL_A, line: 9 },
        { level: "error", source: "network", text: "Failed to load resource: 404", cut: false, url: `${ORIGIN}/missing.png`, line: 0 },
      ],
    },
  });
  assert.deepEqual(Object.keys(reply), ["v", "result"]);
  assert.deepEqual(Object.keys(reply.result), ["origin", "omitted", "entries"]);
});

test("levels keeps every entry, the warnings and errors, or the errors alone, and each domain's level words map to the same four", async () => {
  const events = {
    console: [
      ["Console.messageAdded", said("debug", "d")],
      ["Console.messageAdded", said("log", "l")],
      ["Console.messageAdded", said("info", "i")],
      ["Console.messageAdded", said("warning", "w")],
      ["Console.messageAdded", said("error", "e")],
    ],
    log: [
      ["Log.entryAdded", logged("verbose", "v")],
      ["Log.entryAdded", logged("info", "i2")],
      ["Log.entryAdded", logged("warning", "w2")],
      ["Log.entryAdded", logged("error", "e2")],
    ],
  };
  const kept = async (levels) => entriesOf(await run(rig(events), { levels })).map((entry) => entry.text);
  assert.deepEqual(await kept("all"), ["d", "l", "i", "w", "e", "v", "i2", "w2", "e2"]);
  assert.deepEqual(await kept("warnings_and_errors"), ["w", "e", "w2", "e2"]);
  assert.deepEqual(await kept("errors"), ["e", "e2"]);
  const levels = entriesOf(await run(rig(events))).map((entry) => entry.level);
  assert.deepEqual(levels, ["debug", "log", "info", "warning", "error", "debug", "info", "warning", "error"]);
});

test("no more than max_entries entries are kept, the newest, and omitted counts the older ones of the levels asked for, not the ones a level left out", async () => {
  const told = Array.from({ length: 7 }, (_, at) => ["Console.messageAdded", said(at % 2 ? "error" : "log", `n${at}`)]);
  const everything = await run(rig({ console: told }), { max_entries: 3 });
  assert.deepEqual(entriesOf(everything).map((entry) => entry.text), ["n4", "n5", "n6"]);
  assert.equal(everything.result.omitted, 4);
  const errors = await run(rig({ console: told }), { levels: "errors", max_entries: 2 });
  assert.deepEqual(entriesOf(errors).map((entry) => entry.text), ["n3", "n5"]);
  assert.equal(errors.result.omitted, 1);
  const few = await run(rig({ console: told }), { max_entries: 10 });
  assert.equal(entriesOf(few).length, 7);
  assert.equal(few.result.omitted, 0);
  const defaults = await run(rig({ console: Array.from({ length: 150 }, (_, at) => ["Console.messageAdded", said("log", `m${at}`)]) }));
  assert.equal(entriesOf(defaults).length, DEFAULT_ENTRIES);
  assert.equal(defaults.result.omitted, 50);
  assert.equal(entriesOf(defaults)[0].text, "m50");
});

test("an entry's text and script address are cut to their limits, the text says it was cut, and a level, source or line outside the lists is given the neutral value", () => {
  const long = "é".repeat(TEXT_LIMIT + 5);
  const cutText = entryOf("console", { source: "console-api", level: "log", text: long, url: "u".repeat(URL_LIMIT + 9), line: 4 });
  assert.equal([...cutText.text].length, TEXT_LIMIT);
  assert.equal(cutText.cut, true);
  assert.equal(cutText.url.length, URL_LIMIT);
  const exact = entryOf("console", { source: "console-api", level: "log", text: "é".repeat(TEXT_LIMIT) });
  assert.equal(exact.cut, false);
  assert.equal([...exact.text].length, TEXT_LIMIT);
  // A character outside the basic plane counts once, and is not split.
  const astral = entryOf("console", { level: "log", text: "😀".repeat(TEXT_LIMIT + 1) });
  assert.equal([...astral.text].length, TEXT_LIMIT);
  assert.ok(!astral.text.includes("�"));
  const odd = entryOf("log", { source: "made-up", level: "fatal", text: 12, url: ["x"], lineNumber: -1 });
  assert.deepEqual(odd, { level: "log", source: "other", text: "", cut: false, url: "", line: 0 });
  for (const line of [1.5, "3", NaN, Infinity, 2 ** 60, null, undefined]) {
    assert.equal(entryOf("console", { level: "log", text: "t", line }).line, 0, String(line));
  }
  assert.equal(entryOf("log", { level: "info", text: "t", lineNumber: 12 }).line, 12);
  for (const nothing of [null, undefined, "text", 5, true]) {
    assert.equal(entryOf("console", nothing), null);
  }
  assert.ok(Object.isFrozen(cutText));
});

test("an entry of the report is made only of the fields of the closed shape, whatever the browser's event held", async () => {
  const r = rig({
    console: [["Console.messageAdded", said("log", "t", { toString: "x", __proto__: { evil: 1 }, constructor: "c", extra: { deep: 1 } })]],
  });
  const [entry] = entriesOf(await run(r));
  assert.deepEqual(Object.keys(entry).sort(), ["cut", "level", "line", "source", "text", "url"]);
  assert.ok(isReport({ origin: ORIGIN, omitted: 0, entries: [entry] }));
});

test("only the events of the tab that is read are collected", async () => {
  let fake;
  const r = rig({
    answers: {
      "Console.enable": () => {
        fake.emit("Console.messageAdded", said("error", "other tab"), TAB + 1);
        fake.emit("Console.messageAdded", said("error", "this tab"), TAB);
        fake.emit("Log.entryAdded", logged("error", "other tab too"), undefined);
        return {};
      },
    },
  });
  fake = r.fake;
  const reply = await run(r);
  assert.deepEqual(entriesOf(reply).map((entry) => entry.text), ["this tab"]);
});

test("an event that comes after the read ended, or before the domains were enabled, is not collected", async () => {
  const fake = fakeDebugger({ targets: [pageAt(URL_A, TAB)] });
  const heard = [];
  mock.timers.enable({ apis: ["setTimeout"] });
  const token = Object.freeze({});
  const reading = withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      fake.emit("Console.messageAdded", said("log", "before"), TAB);
      const read = readConsole(session, token, (domain, raw) => heard.push(raw.text));
      await settle();
      fake.emit("Log.entryAdded", logged("error", "during"), TAB);
      mock.timers.tick(CONSOLE_SETTLE_MS);
      await read;
      fake.emit("Console.messageAdded", said("log", "after"), TAB);
    },
    { allows: (given) => given === token },
  );
  await reading;
  assert.deepEqual(heard, ["during"]);
});

test("the report waits a short time after the browser's answer for what comes with it, and not less", async () => {
  const fake = fakeDebugger({ targets: [pageAt(URL_A, TAB)] });
  mock.timers.enable({ apis: ["setTimeout"] });
  const token = Object.freeze({});
  let ended = false;
  const reading = withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      await readConsole(session, token, () => {});
      ended = true;
    },
    { allows: (given) => given === token },
  );
  await settle();
  mock.timers.tick(CONSOLE_SETTLE_MS - 1);
  await settle();
  assert.equal(ended, false, "it stopped early");
  mock.timers.tick(1);
  await reading;
  assert.equal(ended, true);
});

test("a page that left the address while the console was read is not reported", async () => {
  const r = rig({ frame: `${ORIGIN}/elsewhere`, console: [["Console.messageAdded", said("error", "from another page")]] });
  assert.deepEqual(await run(r), failed("left"));
  assert.equal(detaches(r.fake), 1);
  const same = rig({ frame: URL_A });
  assert.equal((await run(same)).result.origin, ORIGIN);
});

test("a console that is never answered ends the call at its limit, detaches, and leaves the extension free for the next request", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  const r = rig({ answers: { "Log.enable": () => new Promise(() => {}) } });
  const waiting = answer(request(), r.fake.chrome);
  await settle();
  mock.timers.tick(READ_WAIT_MS - 1);
  await settle();
  assert.equal(detaches(r.fake), 0, "the call ended early");
  mock.timers.tick(1);
  assert.deepEqual(await waiting, failed("failed"));
  assert.equal(detaches(r.fake), 1);
  assert.equal(r.fake.attached.size, 0);
  const next = rig();
  assert.ok((await run(next)).result);
});

test("the debugger is detached after every path of the call after the Allow", async () => {
  const throwing = (method) => (asked) => (asked === method ? new Error(`${method} failed`) : undefined);
  for (const method of ["Page.enable", "Console.enable", "Log.enable", "Page.getFrameTree"]) {
    const r = rig({ failOn: { sendCommand: throwing(method) } });
    assert.deepEqual(await run(r), failed("failed"), method);
    assert.equal(callsOf(r.fake, "attach").length, 1, method);
    assert.equal(detaches(r.fake), 1, method);
    assert.equal(r.fake.attached.size, 0, method);
  }
  const noAttach = rig({ failOn: { attach: new Error("another debugger") } });
  assert.deepEqual(await run(noAttach), failed("failed"));
  assert.equal(detaches(noAttach.fake), 0);
});

test("no tab, or several, at the address ends the call before the person is asked or a debugger is attached", async () => {
  const none = rig();
  none.fake.chrome.debugger.getTargets = async () => [];
  assert.deepEqual(await run(none), failed("tabs", 0));
  const two = fakeDebugger({ targets: [pageAt(URL_A, 1), pageAt(URL_A, 2)] });
  let asked = 0;
  setConfirmation(async () => {
    asked += 1;
    return null;
  });
  assert.deepEqual(await run({ fake: two }), failed("tabs", 2));
  assert.equal(asked, 0);
  assert.equal(callsOf(two, "attach").length, 0);
  assert.equal(none.windows.length, 0);
});

test("the request takes the address, levels and max_entries and nothing else, and a bad one is refused before the browser is asked", async () => {
  const r = rig();
  for (const extra of [
    { role: "button" },
    { selector: "body" },
    { tabId: TAB },
    { x: 1 },
    { filter: "error" },
    { since: 0 },
    { expression: "1" },
  ]) {
    assert.deepEqual(await answer(request(extra), r.fake.chrome), refused("extra"), JSON.stringify(extra));
  }
  for (const bad of [
    { levels: "warnings" },
    { levels: "ERRORS" },
    { levels: ["errors"] },
    { levels: "" },
    { levels: 3 },
    { max_entries: 0 },
    { max_entries: MAX_ENTRIES + 1 },
    { max_entries: 1.5 },
    { max_entries: "5" },
    { max_entries: -1 },
    { max_entries: null },
  ]) {
    const [name] = Object.keys(bad);
    assert.deepEqual(await answer(request(bad), r.fake.chrome), refused("argument", name), JSON.stringify(bad));
  }
  for (const url of [undefined, "", "file:///etc/passwd", "chrome://version", "javascript:1", 5]) {
    const message = { v: 1, method: METHOD, params: url === undefined ? {} : { url } };
    assert.deepEqual(await answer(message, r.fake.chrome), refused("argument", "url"), String(url));
  }
  assert.deepEqual(r.fake.calls, []);
  assert.deepEqual(r.windows, []);
  assert.deepEqual(METHODS.read_console.required, ["url"]);
  assert.deepEqual({ ...METHODS.read_console.optional }, { levels: "all", max_entries: 100 });
  assert.deepEqual([...LEVELS], ["all", "warnings_and_errors", "errors"]);
  const accepted = await run(rig(), { levels: "errors", max_entries: MAX_ENTRIES });
  assert.ok(accepted.result);
  const least = await run(rig(), { max_entries: 1 });
  assert.ok(least.result);
});

test("the reply holds the report and nothing else: a handler's extra members, a report that is not of the closed shape and an origin that is not one are the generic failure", async () => {
  const entry = { level: "log", source: "console-api", text: "t", cut: false, url: "", line: 0 };
  const good = { origin: ORIGIN, omitted: 0, entries: [entry] };
  const ask = (outcome) => answer(request(), "the browser", { read_console: async () => outcome });
  const extra = await ask({ ...good, title: "A page", tabId: 4, html: "<p>" });
  assert.deepEqual(extra, failed("failed"));
  assert.deepEqual(await ask({ ...good, entries: [{ ...entry, stack: "x" }] }), failed("failed"));
  const shapes = [
    undefined,
    null,
    "text",
    [],
    { ...good, omitted: -1 },
    { ...good, omitted: 1.5 },
    { ...good, entries: "x" },
    { ...good, entries: Array.from({ length: MAX_ENTRIES + 1 }, () => entry) },
    { ...good, entries: [{ ...entry, level: "fatal" }] },
    { ...good, entries: [{ ...entry, source: "page" }] },
    { ...good, entries: [{ ...entry, text: "x".repeat(TEXT_LIMIT + 1) }] },
    { ...good, entries: [{ ...entry, url: "x".repeat(URL_LIMIT + 1) }] },
    { ...good, entries: [{ ...entry, cut: "no" }] },
    { ...good, entries: [{ ...entry, line: -1 }] },
    { ...good, entries: [{ ...entry, line: 1.5 }] },
    { ...good, entries: [{ level: "log", source: "console-api", text: "t", cut: false, url: "" }] },
    { ...good, entries: [null] },
    { ...good, entries: [[entry]] },
  ];
  for (const shape of shapes) {
    assert.deepEqual(await ask(shape), failed("failed"), JSON.stringify(shape)?.slice(0, 80));
    assert.equal(isReport(shape), false);
  }
  for (const origin of [`${ORIGIN}/page`, "file:///", "", 5, null]) {
    assert.deepEqual(await ask({ ...good, origin }), failed("failed"), String(origin));
  }
  const fine = await ask({ ...good, entries: Array.from({ length: MAX_ENTRIES }, () => entry) });
  assert.equal(fine.result.entries.length, MAX_ENTRIES);
  assert.ok(isReport(good));
  assert.ok(isReport({ ...good, entries: [] }));
  assert.deepEqual([...REPORTS], ["read_console"]);
  // A method that is not of REPORTS keeps replying with a status word and an origin.
  const click = await answer(
    { v: 1, method: "hover_element", params: { url: URL_A, role: "button", name: "x" } },
    "the browser",
    { hover_element: async () => good },
  );
  assert.deepEqual(click, failed("failed"));
});

test("the window says what is read in words of its own for each choice of levels", () => {
  assert.equal(limitsText("all", 100), "every entry, the newest 100 at most");
  assert.equal(limitsText("warnings_and_errors", 5), "warnings and errors, the newest 5 at most");
  assert.equal(limitsText("errors", 1), "errors, the newest 1 at most");
  for (const levels of LEVELS) {
    assert.match(limitsText(levels, 3), /, the newest 3 at most$/);
  }
});

test("the lists of levels and sources hold the browser's words for both domains, and no Runtime method reports the console", () => {
  assert.deepEqual([...LEVEL_WORDS], ["debug", "log", "info", "warning", "error"]);
  for (const source of ["console-api", "javascript", "network", "deprecation", "violation", "intervention", "other"]) {
    assert.ok(SOURCES.includes(source), source);
  }
  const CODE = scripts();
  assert.deepEqual(occurrences(CODE, "\\bRuntime\\b"), {});
  // The domains are named, and their events listened for, in cdp.js alone.
  for (const name of ["Console\\.enable", "Log\\.enable", "Console\\.messageAdded", "Log\\.entryAdded"]) {
    assert.deepEqual(Object.keys(occurrences(CODE, name)), ["cdp.js"], name);
  }
});

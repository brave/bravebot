// The tool of the console-trusted group, which this extension checks and sends to
// the BraveBot Trusted Input extension, and whose reply is the page's console as
// text: what has to hold before anything is sent, what is sent, what shape of
// reply is passed on and what text is made of it, and that every other tool's
// reply is still a word. Run once against the other extension's own code, over a
// fake debugger.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { activityLog } from "../activity.js";
import {
  DEFAULT_ENTRIES,
  DEFAULT_LEVELS,
  LEVELS,
  LEVEL_WORDS,
  MAX_ENTRIES,
  SOURCES,
  TEXT_LIMIT,
  URL_LIMIT,
  isReport,
  reportText,
} from "../consoles.js";
import { NO_ACCESS } from "../groups.js";
import { RUNS_KEY } from "../lifetimes.js";
import { setTool, turnOn } from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  BUSY_MESSAGE,
  GROUP_SWITCHES_KEY,
  SITE_SCOPES_KEY,
  handle,
} from "../tools.js";
import {
  NOT_THERE,
  NOT_UNDERSTOOD,
  REPORT_METHODS,
  SECOND_ID,
  STOPPED,
  TOO_LATE,
  VERSION,
  WAIT_MS,
  forward,
} from "../trusted.js";
import { approve, setConfirmation, CONFIRMATION_WAIT_MS } from "../../extension-trusted/approval.js";
import { CONSOLE_SETTLE_MS } from "../../extension-trusted/cdp.js";
import * as seconds from "../../extension-trusted/consoles.js";
import { answer, REPORTS } from "../../extension-trusted/requests.js";
import { fakeDebugger, pageAt } from "../../extension-trusted/tests/fake-debugger.mjs";
import { fakeChrome } from "./fake-chrome.mjs";

afterEach(() => {
  mock.timers.reset();
  setConfirmation();
});

const NEWS = "https://news.example";
const PAGE = `${NEWS}/front`;
const BANK = "https://bank.example";
const METHOD = "read_console";
const CALL = { url: PAGE };

const entry = (more = {}) => ({
  level: "error",
  source: "console-api",
  text: "boom",
  cut: false,
  url: `${PAGE}.js`,
  line: 12,
  ...more,
});
const report = (entries = [entry()], more = {}) => ({ origin: NEWS, omitted: 0, entries, ...more });
const reply = (result = report()) => ({ v: VERSION, result });

// A browser in which the person turned console-trusted on and allowed the sites
// in `allowed`, and in which the other extension answers each message with
// `answered`: a reply, or a function of the id sent to and the request that gives
// one or throws. `sent` lists every message that was sent.
async function trusted({ allowed = [NEWS], blocked = [], answered = reply(), local = {} } = {}) {
  const chrome = fakeChrome({
    held: {},
    local: { [ALLOWED_SITES_KEY]: allowed, [BLOCKED_SITES_KEY]: blocked, ...local },
    tabs: [{ id: 1, windowId: 1, title: "News", url: PAGE }],
  });
  chrome.sent = [];
  chrome.runtime.sendMessage = async (id, request) => {
    chrome.sent.push([id, request]);
    return typeof answered === "function" ? answered(id, request) : answered;
  };
  assert.equal(await turnOn(chrome, "console-trusted", NO_ACCESS), true);
  return chrome;
}

const call = (chrome, method, params) => handle({ id: 7, method, params }, chrome);

// Lets what is waiting on a resolved promise run.
async function settle() {
  for (let turn = 0; turn < 50; turn += 1) {
    await Promise.resolve();
  }
}

test("the words, the limits and the table of the console tool are the same in both extensions", () => {
  assert.deepEqual([...LEVELS], [...seconds.LEVELS]);
  assert.deepEqual([...LEVELS], ["all", "warnings_and_errors", "errors"]);
  assert.equal(DEFAULT_LEVELS, seconds.DEFAULT_LEVELS);
  assert.equal(MAX_ENTRIES, seconds.MAX_ENTRIES);
  assert.equal(DEFAULT_ENTRIES, seconds.DEFAULT_ENTRIES);
  assert.equal(TEXT_LIMIT, seconds.TEXT_LIMIT);
  assert.equal(URL_LIMIT, seconds.URL_LIMIT);
  assert.deepEqual([...LEVEL_WORDS], [...seconds.LEVEL_WORDS]);
  assert.deepEqual([...SOURCES], [...seconds.SOURCES]);
  assert.deepEqual([...REPORT_METHODS], [METHOD]);
  assert.deepEqual([...REPORT_METHODS], [...REPORTS]);
  assert.equal(MAX_ENTRIES, 200);
  assert.equal(DEFAULT_ENTRIES, 100);
});

test("the work after the person's answer fits in what is left of the wait for the other extension, with room before it", () => {
  assert.ok(CONFIRMATION_WAIT_MS + seconds.READ_WAIT_MS < WAIT_MS);
  assert.ok(WAIT_MS - CONFIRMATION_WAIT_MS - seconds.READ_WAIT_MS >= 5_000);
});

test("a call sends the address, the levels and the number of entries and no other argument, to the other extension, and returns the text of its console and nothing else", async () => {
  const chrome = await trusted();
  const got = await call(chrome, METHOD, { ...CALL, selector: "#a", x: 1, tab_id: 1, since: 0, expression: "1" });
  assert.equal(got.error, undefined, got.error?.message);
  assert.deepEqual(chrome.sent, [
    [SECOND_ID, { v: VERSION, method: METHOD, params: { url: PAGE, levels: "all", max_entries: 100 } }],
  ]);
  assert.equal(typeof got.result, "string");
  const [header, line, ...rest] = got.result.split("\n");
  assert.deepEqual(rest, []);
  assert.deepEqual(JSON.parse(header), { origin: NEWS, entries: 1, omitted: 0 });
  assert.deepEqual(JSON.parse(line), entry());

  const asked = await trusted();
  await call(asked, METHOD, { ...CALL, levels: "errors", max_entries: 5 });
  assert.deepEqual(asked.sent[0][1].params, { url: PAGE, levels: "errors", max_entries: 5 });
});

test("a levels or a max_entries outside the lists and bounds is refused before anything is sent", async () => {
  for (const bad of [
    { levels: "warnings" },
    { levels: "ERRORS" },
    { levels: ["errors"] },
    { levels: 1 },
    { max_entries: 0 },
    { max_entries: MAX_ENTRIES + 1 },
    { max_entries: 2.5 },
    { max_entries: "9" },
  ]) {
    const chrome = await trusted();
    const got = await call(chrome, METHOD, { ...CALL, ...bad });
    assert.match(got.error.message, /levels must be one of|max_entries must be a whole number from 1 to 200/, JSON.stringify(bad));
    assert.deepEqual(chrome.sent, [], JSON.stringify(bad));
  }
  const edge = await trusted();
  assert.equal((await call(edge, METHOD, { ...CALL, max_entries: MAX_ENTRIES })).error, undefined);
  assert.equal((await call(edge, METHOD, { ...CALL, max_entries: 1 })).error, undefined);
});

// The checks come first. A forward that came before any of them would send a
// request for a group that is off, a tool that is off, a group whose time has
// ended, a site nobody allowed or a site that is blocked.
test("nothing is sent while the group is off, the tool is off, the lifetime has ended, the site is not allowed or the site is blocked", async () => {
  const off = await trusted();
  off.local[GROUP_SWITCHES_KEY] = {};
  assert.match((await call(off, METHOD, CALL)).error.message, /in the console-trusted group, which is turned off/);
  assert.deepEqual(off.sent, []);

  const toolOff = await trusted();
  await setTool(toolOff, METHOD, false);
  assert.match((await call(toolOff, METHOD, CALL)).error.message, /is turned off in the BraveBot extension's options/);
  assert.deepEqual(toolOff.sent, []);

  const lapsed = await trusted();
  lapsed.session[RUNS_KEY] = {};
  assert.match((await call(lapsed, METHOD, CALL)).error.message, /was turned on until Brave restarts, and that has ended/);
  assert.deepEqual(lapsed.sent, []);

  const elsewhere = await trusted({ allowed: [BANK] });
  assert.match((await call(elsewhere, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(elsewhere.sent, []);

  const blocked = await trusted({ allowed: [NEWS], blocked: [NEWS] });
  assert.match((await call(blocked, METHOD, CALL)).error.message, /in the blocked sites list/);
  assert.deepEqual(blocked.sent, []);

  for (const url of [`blob:${NEWS}/0b1c2d3e`, `view-source:${PAGE}`, "about:blank", "", undefined]) {
    const chrome = await trusted({ allowed: [NEWS, "about://blank"] });
    assert.ok((await call(chrome, METHOD, { url })).error, String(url));
    assert.deepEqual(chrome.sent, [], String(url));
  }
});

test("the group is limited to the allowed sites whatever scope is stored, and an allowed list that cannot be read allows none", async () => {
  const widened = await trusted({
    allowed: [BANK],
    local: { [SITE_SCOPES_KEY]: { "console-trusted": "all", "page-read": "all" } },
  });
  assert.match((await call(widened, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(widened.sent, []);

  const unreadable = await trusted({ allowed: "https://news.example" });
  assert.match((await call(unreadable, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(unreadable.sent, []);
});

// A console is passed on only when it is the one shape a console has. Each of
// these replies is one a wrong check would pass: a member the report does not
// have, an entry with another member, a level or a source off the list, text over
// the limit, and an origin that is not one.
test("a reply is passed on only when it is of the closed shape, and each reply that is not is not understood and none of it is passed on", async () => {
  const cases = {
    "a member beside the report": reply(report([entry()], { title: "News" })),
    "no origin": reply({ omitted: 0, entries: [entry()] }),
    "no omitted": reply({ origin: NEWS, entries: [entry()] }),
    "no entries": reply({ origin: NEWS, omitted: 0 }),
    "an origin that is a path": reply(report([entry()], { origin: PAGE })),
    "an origin that is not on the web": reply(report([entry()], { origin: "file:///" })),
    "an origin that is not text": reply(report([entry()], { origin: 5 })),
    "an omitted that is negative": reply(report([entry()], { omitted: -1 })),
    "an omitted that is not whole": reply(report([entry()], { omitted: 1.5 })),
    "entries that are a string": reply(report("boom")),
    "entries that are an object": reply(report({ 0: entry() })),
    "more entries than the most": reply(report(Array.from({ length: MAX_ENTRIES + 1 }, () => entry()))),
    "an entry that is a string": reply(report(["boom"])),
    "an entry that is null": reply(report([null])),
    "an entry that is a list": reply(report([[entry()]])),
    "a member beside the entry's": reply(report([entry({ stack: "at x" })])),
    "an entry with no line": reply(report([(({ line, ...rest }) => rest)(entry())])),
    "an entry with no cut": reply(report([(({ cut, ...rest }) => rest)(entry())])),
    "a level off the list": reply(report([entry({ level: "fatal" })])),
    "a level in another case": reply(report([entry({ level: "Error" })])),
    "a source off the list": reply(report([entry({ source: "page" })])),
    "text that is not a string": reply(report([entry({ text: 5 })])),
    "text a character over": reply(report([entry({ text: "x".repeat(TEXT_LIMIT + 1) })])),
    "a script address a character over": reply(report([entry({ url: "x".repeat(URL_LIMIT + 1) })])),
    "a cut that is not a boolean": reply(report([entry({ cut: 1 })])),
    "a line that is negative": reply(report([entry({ line: -1 })])),
    "a line that is not whole": reply(report([entry({ line: 1.5 })])),
    "a line that is text": reply(report([entry({ line: "3" })])),
    "a status in place of a report": reply({ status: "clicked", origin: NEWS }),
    "a status beside the report": reply({ ...report(), status: "clicked" }),
    "a picture in place of a report": reply({ image: { mimeType: "image/jpeg", data: "QQ==" } }),
    "an empty result": reply({}),
    "a result that is a string": reply("boom"),
    "a result with an error beside it": { ...reply(), error: { kind: "failed", message: "x" } },
    "a version that is not ours": { v: VERSION + 1, result: report() },
  };
  for (const [label, answered] of Object.entries(cases)) {
    const chrome = await trusted({ answered });
    const got = await call(chrome, METHOD, CALL);
    assert.equal(got.error?.message, NOT_UNDERSTOOD, label);
    assert.equal(got.result, undefined, label);
    assert.equal(JSON.stringify(got).includes("boom"), false, `${label}: the page's text was passed on`);
  }
  // The shape of the report is checked by isReport, and the origin by the caller.
  for (const answered of [cases["a member beside the report"], cases["an entry with no line"], cases["a level off the list"]]) {
    assert.equal(isReport(answered.result), false);
  }
  // The most is exact, and the limits are exact.
  const edge = await trusted({
    answered: reply(
      report(
        Array.from({ length: MAX_ENTRIES }, () => entry({ text: "é".repeat(TEXT_LIMIT), url: "u".repeat(URL_LIMIT) })),
      ),
    ),
  });
  const taken = await call(edge, METHOD, CALL);
  assert.equal(taken.result.split("\n").length, MAX_ENTRIES + 1);
  assert.deepEqual(JSON.parse(taken.result.split("\n")[0]), { origin: NEWS, entries: MAX_ENTRIES, omitted: 0 });
  const none = await trusted({ answered: reply(report([], { omitted: 7 })) });
  assert.deepEqual(JSON.parse((await call(none, METHOD, CALL)).result), { origin: NEWS, entries: 0, omitted: 7 });
});

test("the tool's own table and no reply says which tool's reply is the console: a console for any other tool, and a status for this one, are not understood", async () => {
  for (const method of ["click_element_trusted", "hover_element"]) {
    const chrome = await trusted({ answered: reply() });
    const got = await forward(chrome, method, { url: PAGE, role: "link", name: "Docs" }).catch((error) => error);
    assert.equal(got.message, NOT_UNDERSTOOD, method);
  }
  const status = await trusted({ answered: reply({ status: "hovered", origin: NEWS }) });
  const got = await forward(status, METHOD, CALL).catch((error) => error);
  assert.equal(got.message, NOT_UNDERSTOOD);
  // The ordinary tools still answer a status word and the origin.
  const ordinary = await trusted({ answered: reply({ status: "hovered", origin: NEWS }) });
  assert.deepEqual(await forward(ordinary, "hover_element", { url: PAGE, role: "link", name: "Docs" }), {
    status: "hovered",
    origin: NEWS,
  });
});

// The text is one JSON object on each line. A page chooses every word of an entry,
// so none of them can end a line, start another, hide the rest of one, change how
// it is drawn, or be mistaken for an escape this code wrote.
test("the page's words are written so that they cannot end a line, forge another, or draw differently, and each line reads back as the entry it was made from", async () => {
  const hostile = [
    'x"}\n{"level":"error","source":"console-api","text":"forged","cut":false,"url":"","line":1}',
    "a‮b‬",
    "a b c\u0085d",
    "a​b­c⁦d⁩",
    "tab\there\rcarriage\u0000nul\u007fdel",
    "\\u202e is text, not a character",
    " private \u{f0001} plane",
    " 　⠀ blanks",
    "😀 and é and 日本語",
    "lone \ud800 surrogate",
  ];
  const entries = hostile.map((text, at) => entry({ text, url: `https://evil.example/${text}`, line: at }));
  const chrome = await trusted({ answered: reply(report(entries)) });
  const got = (await call(chrome, METHOD, CALL)).result;
  const lines = got.split("\n");
  assert.equal(lines.length, hostile.length + 1, "a line was added or lost");
  // Nothing in a line is a character a person cannot see.
  for (const line of lines) {
    assert.doesNotMatch(line, /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\p{Co}\p{Cs}   -   　ᅟᅠㅤﾠ⠀]/u, JSON.stringify(line));
  }
  // Each line reads back as what it was made from, and an escape written by
  // this code is told from the page's own text that spells one.
  lines.slice(1).forEach((line, at) => {
    const read = JSON.parse(line);
    const expected = hostile[at];
    assert.equal(read.text, expected, `entry ${at}`);
    assert.equal(read.url, `https://evil.example/${expected}`, `entry ${at}`);
    assert.equal(read.line, at);
    assert.deepEqual(Object.keys(read), ["level", "source", "text", "cut", "url", "line"]);
  });
  assert.notEqual(lines[2], lines[8]);
  assert.match(lines[2], /a\\u202eb\\u202c/);
  assert.match(lines[6], /\\\\u202e is text/);
  assert.match(lines[7], /\\ue000 private \\udb80\\udc01 plane/);
});

test("the text of a report is a header line and a line for each entry in the order they came, and a report that isReport rejects is not for reportText", () => {
  const text = reportText(report([entry({ text: "one" }), entry({ text: "two", level: "warning", line: 0 })], { omitted: 3 }));
  const [header, one, two] = text.split("\n").map((line) => JSON.parse(line));
  assert.deepEqual(header, { origin: NEWS, entries: 2, omitted: 3 });
  assert.equal(one.text, "one");
  assert.equal(two.text, "two");
  assert.equal(two.level, "warning");
  assert.equal(isReport(report()), true);
  assert.equal(isReport(null), false);
  assert.equal(isReport([]), false);
});

test("an error of the other extension is a refusal or a failure with its sentence after our own, for the console tool as for the others", async () => {
  const refused = await trusted({
    answered: { v: VERSION, error: { kind: "refusal", message: "it was denied in the confirmation window, so nothing was done" } },
  });
  const denied = await call(refused, METHOD, CALL);
  assert.equal(
    denied.error.message,
    "the BraveBot Trusted Input extension refused it: it was denied in the confirmation window, so nothing was done",
  );
  assert.equal((await activityLog(refused)).at(-1).outcome, "refused");

  const failed = await trusted({
    answered: { v: VERSION, error: { kind: "failed", message: "the page left the URL before it was acted on" } },
  });
  const left = await call(failed, METHOD, CALL);
  assert.match(left.error.message, /^the BraveBot Trusted Input extension could not do it: the page left the URL/);
  assert.equal((await activityLog(failed)).at(-1).outcome, "failed");
});

test("an extension that is not there is one sentence of refusal, a worker that stopped is a failure, and a reply that does not come ends the call at the wait", async () => {
  const absent = await trusted({
    answered: () => {
      throw new Error("Could not establish connection. Receiving end does not exist.");
    },
  });
  assert.equal((await call(absent, METHOD, CALL)).error.message, NOT_THERE);
  assert.equal((await activityLog(absent)).at(-1).outcome, "refused");

  const stopped = await trusted({
    answered: () => {
      throw new Error("The message port closed before a response was received.");
    },
  });
  assert.equal((await call(stopped, METHOD, CALL)).error.message, STOPPED);

  mock.timers.enable({ apis: ["setTimeout"] });
  const silent = await trusted();
  silent.runtime.sendMessage = () => new Promise(() => {});
  let outcome;
  forward(silent, METHOD, CALL).then(
    () => (outcome = "done"),
    (error) => (outcome = error),
  );
  await settle();
  mock.timers.tick(WAIT_MS - 1);
  await settle();
  assert.equal(outcome, undefined);
  mock.timers.tick(1);
  await settle();
  assert.equal(outcome.message, TOO_LATE);
});

test("while the console is read no other action is started, and the place is free again however the call ended", async () => {
  const chrome = await trusted();
  let release;
  chrome.runtime.sendMessage = (id, request) => {
    chrome.sent.push([id, request]);
    return new Promise((resolve) => {
      release = resolve;
    });
  };
  const first = call(chrome, METHOD, CALL);
  await settle();
  assert.equal(chrome.sent.length, 1);
  assert.equal((await call(chrome, METHOD, CALL)).error.message, BUSY_MESSAGE);
  assert.equal(chrome.sent.length, 1, "a second request was sent");
  release(reply());
  assert.equal(typeof (await first).result, "string");
  chrome.runtime.sendMessage = async () => {
    throw new Error("The message port closed before a response was received.");
  };
  assert.equal((await call(chrome, METHOD, CALL)).error.message, STOPPED);
  chrome.runtime.sendMessage = async () => reply();
  assert.equal(typeof (await call(chrome, METHOD, CALL)).result, "string");
});

test("the activity log keeps the group, the tool, the outcome and the origin and nothing of the console", async () => {
  const chrome = await trusted({ answered: reply(report([entry({ text: "secret-token-1234" })])) });
  const got = await call(chrome, METHOD, { ...CALL, levels: "errors" });
  assert.match(got.result, /secret-token-1234/);
  const log = await activityLog(chrome);
  assert.deepEqual(
    log.map(({ group, tool, outcome, origins }) => ({ group, tool, outcome, origins })),
    [{ group: "console-trusted", tool: METHOD, outcome: "ran", origins: [NEWS] }],
  );
  assert.equal(JSON.stringify(log).includes("secret-token-1234"), false);
  assert.equal(JSON.stringify(log).includes("/front"), false);
  assert.equal(JSON.stringify(log).includes("errors"), false);
});

// The whole chain in this process: this extension's call, the other extension's
// request check and tool, over a fake debugger, and the window answered as the
// person would. What arrives is the text of what the debugger reported.
test("a call through the other extension's own code returns the console the browser reported, and a denial returns a refusal and attaches to nothing", async () => {
  let fake;
  fake = fakeDebugger({
    targets: [pageAt(PAGE, 1)],
    answers: {
      "Console.enable": () => {
        fake.emit("Console.messageAdded", { message: { source: "console-api", level: "log", text: "hello", url: PAGE, line: 4, column: 1 } }, 1);
        fake.emit("Console.messageAdded", { message: { source: "console-api", level: "error", text: "broke‮", url: PAGE, line: 5, column: 1 } }, 1);
        return {};
      },
      "Log.enable": () => {
        fake.emit("Log.entryAdded", { entry: { source: "network", level: "error", text: "Failed to load resource", url: `${NEWS}/x.png`, timestamp: 1 } }, 1);
        return {};
      },
      "Page.getFrameTree": () => ({ frameTree: { frame: { url: PAGE } } }),
    },
  });
  const chrome = await trusted({ answered: (id, request) => answer(request, fake.chrome) });
  const asked = [];
  setConfirmation(async (resolution) => {
    asked.push(resolution.description);
    return approve(resolution);
  });
  mock.timers.enable({ apis: ["setTimeout"] });
  const running = call(chrome, METHOD, { ...CALL, levels: "warnings_and_errors" });
  await settle();
  await new Promise((resolve) => setImmediate(resolve));
  mock.timers.tick(CONSOLE_SETTLE_MS);
  const got = await running;
  assert.equal(got.error, undefined, got.error?.message);
  const lines = got.result.split("\n").map((line) => JSON.parse(line));
  assert.deepEqual(lines[0], { origin: NEWS, entries: 2, omitted: 0 });
  assert.deepEqual(lines.slice(1).map(({ text }) => text), ["broke‮", "Failed to load resource"]);
  assert.match(got.result, /broke\\u202e/);
  assert.equal(asked.length, 1);
  assert.equal(asked[0].action, "console");
  assert.equal(asked[0].limits, "warnings and errors, the newest 100 at most");
  assert.equal(fake.attached.size, 0);

  mock.timers.reset();
  const attaches = fake.calls.filter((entry) => entry.call === "attach").length;
  setConfirmation(async () => null);
  const denied = await call(chrome, METHOD, CALL);
  assert.match(denied.error.message, /refused it: it was denied in the confirmation window, so nothing was done$/);
  assert.equal(fake.calls.filter((entry) => entry.call === "attach").length, attaches, "the debugger was attached after a denial");
});

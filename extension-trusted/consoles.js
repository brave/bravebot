// The tool read_console: the entries of the console of the one tab at an
// address, which are what the page's scripts wrote with console.log and the
// others, the errors they threw, and what the browser reported about the page,
// such as a request that failed or a rule of the page that was broken. The
// result is the page's own words as text, and this extension reads none of it
// to decide anything.
//
// The person is asked first, in the confirmation window, which shows the page's
// origin and address and how much is read and lists no element. The debugger is
// not attached until they allow it: the tab is found by its address without
// attaching, and a denial attaches to nothing. After the Allow the session
// enables the Console and Log domains of the browser, waits a short time for
// what they report, and detaches. Nothing in the console is read before then.
//
// The entries are bounded here: at most MAX_ENTRIES of them, the newest, each
// with a level and a source from the closed lists below, its text cut to
// TEXT_LIMIT characters and its script's address to URL_LIMIT. A level and a
// source the browser reports that are not in a list are given the neutral word
// of it. The BraveBot extension checks the same bounds again (extension/consoles.js).

import { describe } from "./actions.js";
import { command, readConsole, requireTab, withSession } from "./cdp.js";
import { Failure } from "./sentences.js";

// What the argument `levels` can be: every entry, the warnings and errors, or
// the errors alone.
export const LEVELS = Object.freeze(["all", "warnings_and_errors", "errors"]);
export const DEFAULT_LEVELS = "all";

// How many entries a call keeps, the most it may ask for and what it gets when it
// says nothing.
export const MAX_ENTRIES = 200;
export const DEFAULT_ENTRIES = 100;

// The most characters of an entry's text and of the address of its script.
export const TEXT_LIMIT = 500;
export const URL_LIMIT = 300;

// The levels an entry has, from the least to the most severe, and the first
// level each value of `levels` keeps.
export const LEVEL_WORDS = Object.freeze(["debug", "log", "info", "warning", "error"]);
const FIRST_KEPT = Object.freeze({ all: 0, warnings_and_errors: 3, errors: 4 });

// What each domain calls its levels, as ours. A level the browser reports that is
// not here is "log".
const LEVEL_OF = Object.freeze({
  console: Object.freeze({ log: "log", debug: "debug", info: "info", warning: "warning", error: "error" }),
  log: Object.freeze({ verbose: "debug", info: "info", warning: "warning", error: "error" }),
});

// The sources an entry can be from: the calls of console, and the places the
// browser reports from. A source that is not here is "other".
export const SOURCES = Object.freeze([
  "console-api",
  "javascript",
  "network",
  "xml",
  "storage",
  "appcache",
  "rendering",
  "security",
  "deprecation",
  "worker",
  "violation",
  "intervention",
  "recommendation",
  "other",
]);

// How long the read may take once the person has allowed it, in milliseconds. It
// is inside the time a call has after the person's answer (cdp.js CALL_WAIT_MS),
// which ends the call whatever it is doing, so the read's own end comes first and
// says the generic failure.
export const READ_WAIT_MS = 10_000;

// A field the browser reports as text, or "" when it is not.
const textOf = (value) => (typeof value === "string" ? value : "");

// The first `limit` characters of `text` and whether any were left out. It looks
// at no more of a long text than it keeps.
function cut(text, limit) {
  let kept = "";
  let count = 0;
  for (const character of text) {
    if (count === limit) {
      return { kept, cut: true };
    }
    kept += character;
    count += 1;
  }
  return { kept, cut: false };
}

// The entry an event of the browser is, as the closed shape of a reply, or null
// when the browser sent something that is not an entry. `domain` is "console" or
// "log". Only the level, the source, the text, the address of the script and its
// line are taken.
export function entryOf(domain, raw) {
  if (typeof raw !== "object" || raw === null) {
    return null;
  }
  const text = cut(textOf(raw.text), TEXT_LIMIT);
  const line = raw.line ?? raw.lineNumber;
  return Object.freeze({
    level: LEVEL_OF[domain][raw.level] ?? "log",
    source: SOURCES.includes(raw.source) ? raw.source : "other",
    text: text.kept,
    cut: text.cut,
    url: cut(textOf(raw.url), URL_LIMIT).kept,
    line: Number.isSafeInteger(line) && line >= 0 ? line : 0,
  });
}

// Whether `entry` is an entry of the closed shape: exactly these members, and
// each within its bounds.
export function isEntry(entry) {
  return (
    typeof entry === "object" &&
    entry !== null &&
    !Array.isArray(entry) &&
    Object.keys(entry).sort().join() === "cut,level,line,source,text,url" &&
    LEVEL_WORDS.includes(entry.level) &&
    SOURCES.includes(entry.source) &&
    typeof entry.text === "string" &&
    [...entry.text].length <= TEXT_LIMIT &&
    typeof entry.cut === "boolean" &&
    typeof entry.url === "string" &&
    [...entry.url].length <= URL_LIMIT &&
    Number.isSafeInteger(entry.line) &&
    entry.line >= 0
  );
}

// Whether `report` is what the tool answers with: {origin, omitted, entries} and
// nothing else, with at most MAX_ENTRIES entries of the closed shape.
export function isReport(report) {
  return (
    typeof report === "object" &&
    report !== null &&
    !Array.isArray(report) &&
    Object.keys(report).sort().join() === "entries,omitted,origin" &&
    typeof report.origin === "string" &&
    Number.isSafeInteger(report.omitted) &&
    report.omitted >= 0 &&
    Array.isArray(report.entries) &&
    report.entries.length <= MAX_ENTRIES &&
    report.entries.every(isEntry)
  );
}

// What the window says is read, in words of the window's own.
const LEVEL_PHRASE = Object.freeze({
  all: "every entry",
  warnings_and_errors: "warnings and errors",
  errors: "errors",
});
export const limitsText = (levels, entries) =>
  `${LEVEL_PHRASE[levels]}, the newest ${entries} at most`;

// Keeps the entries of the levels asked for, and no more than `max` of them: the
// newest. `omitted` counts the older ones of those levels that were not kept.
function collector(levels, max) {
  const first = FIRST_KEPT[levels];
  const kept = [];
  let omitted = 0;
  return {
    add(domain, raw) {
      const entry = entryOf(domain, raw);
      if (entry === null || LEVEL_WORDS.indexOf(entry.level) < first) {
        return;
      }
      kept.push(entry);
      if (kept.length > max) {
        kept.shift();
        omitted += 1;
      }
    },
    report: (origin) => ({ origin, omitted, entries: [...kept] }),
  };
}

// The console of the page at `args.url`, as {origin, omitted, entries}, read
// after the person has allowed it in the window. `permit` is the call's approval
// (tools.js permission()), which accepts the token the window makes for the
// resolution it is asked about and no other.
export async function readConsoleOf(chrome, { url, levels, max_entries: max }, permit) {
  const origin = new URL(url).origin;
  // Looking for the tab is a step before the window, and has the time before it.
  await permit.time.within(requireTab(chrome, url));
  const resolution = Object.freeze({
    description: describe("console", { origin, url, limits: limitsText(levels, max) }),
  });
  const token = await permit.ask(resolution);
  const gathered = collector(levels, max);
  await withSession(
    chrome,
    url,
    async (session) => {
      const reading = (async () => {
        await command(session, "Page.enable");
        await readConsole(session, token, gathered.add);
        // The entries are the page's at this address, and not of one it went on to.
        const { frame } = (await command(session, "Page.getFrameTree")).frameTree;
        if (`${frame.url}${frame.urlFragment ?? ""}` !== url) {
          throw new Failure("left");
        }
      })();
      // A read that ends after the call was ended finds the session gone.
      reading.catch(() => {});
      let timer;
      const stuck = new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Failure("failed")), READ_WAIT_MS);
      });
      try {
        await Promise.race([reading, stuck]);
      } finally {
        clearTimeout(timer);
      }
    },
    { allows: permit.allows, time: permit.time },
  );
  return gathered.report(origin);
}

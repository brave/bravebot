// What the console tool of console-trusted returns, and what the reply of the
// BraveBot Trusted Input extension must look like to be passed on as it. The tool
// is read_console (tools.js), which the other extension does (extension-trusted/
// consoles.js): the entries of a tab's console are the page's own words, and they
// reach a session as a result it cannot read directly.
//
// A reply is passed on only when it is of the closed shape: an origin, a count of
// older entries left out, and at most MAX_ENTRIES entries that each hold exactly a
// level and a source from the lists below, text and an address within their
// limits, whether the text was cut and a line number. The text the tool returns
// is made here from those parts: one JSON object on each line, with every
// character a person cannot see written as the \u escape of JSON, so that the
// text on a line cannot end it, hide the rest of it, change how it is drawn or be
// told apart from an escape this code wrote. The words of the lists and the
// limits are the other extension's.

import { shownParts } from "./shown.js";

// What the argument `levels` can be, and what it is when the call says nothing.
export const LEVELS = Object.freeze(["all", "warnings_and_errors", "errors"]);
export const DEFAULT_LEVELS = "all";

// The most entries a call asks for, and what it gets when it says nothing.
export const MAX_ENTRIES = 200;
export const DEFAULT_ENTRIES = 100;

// The most characters of an entry's text and of the address of its script.
export const TEXT_LIMIT = 500;
export const URL_LIMIT = 300;

// The levels an entry has, from the least to the most severe.
export const LEVEL_WORDS = Object.freeze(["debug", "log", "info", "warning", "error"]);

// The places an entry is from: the calls of console, and the places the browser
// reports from.
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

// Whether `value` is an object that holds named members.
const isPlain = (value) =>
  typeof value === "object" && value !== null && !Array.isArray(value);

// Whether `entry` holds exactly the members of an entry, each within its bounds.
function isEntry(entry) {
  return (
    isPlain(entry) &&
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

// Whether `result` holds exactly an origin, a count of entries left out and a
// list of at most MAX_ENTRIES entries. That the origin is one is for the caller.
export function isReport(result) {
  return (
    isPlain(result) &&
    Object.keys(result).sort().join() === "entries,omitted,origin" &&
    typeof result.origin === "string" &&
    Number.isSafeInteger(result.omitted) &&
    result.omitted >= 0 &&
    Array.isArray(result.entries) &&
    result.entries.length <= MAX_ENTRIES &&
    result.entries.every(isEntry)
  );
}

// The \u escape of JSON for the character a mark names ("U+202E"), as two escapes
// for a character beyond the first plane.
function escapeOf(mark) {
  const unit = (number) => `\\u${number.toString(16).padStart(4, "0")}`;
  const code = Number.parseInt(mark.slice(2), 16);
  if (code > 0xffff) {
    const rest = code - 0x10000;
    return unit(0xd800 + (rest >> 10)) + unit(0xdc00 + (rest & 0x3ff));
  }
  return unit(code);
}

// `json` with each character that cannot be seen written as its \u escape. JSON
// text holds only characters of the structure outside its strings, so the
// characters changed are in strings, where an escape means the same character.
function visible(json) {
  const { pieces } = shownParts(json);
  return pieces.map((piece) => (typeof piece === "string" ? piece : escapeOf(piece.mark))).join("");
}

// The text of a report that `isReport` accepted: a line for the origin and the
// counts, and a line for each entry in the order they came.
export function reportText({ origin, omitted, entries }) {
  const lines = [{ origin, entries: entries.length, omitted }];
  for (const { level, source, text, cut, url, line } of entries) {
    lines.push({ level, source, text, cut, url, line });
  }
  return lines.map((line) => visible(JSON.stringify(line))).join("\n");
}

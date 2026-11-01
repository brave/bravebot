// The eight methods this extension answers, the arguments each takes, and the
// shape of a reply. A request is {v, method, params}. The method has to be in
// the table, and every argument has to be one the method takes and be a word of
// the closed lists or a bounded string, before anything else happens. No other
// argument is read, so a selector, a node number or a coordinate in a request is
// refused and never used.
//
// A reply is {v, result: {status, origin}} or {v, error: {kind, message}}. The
// status is a word of STATUSES, the origin is the origin of the address the
// request named, and the message is a sentence of sentences.js. Nothing of the
// page and nothing the request wrote is in a reply. There are two exceptions. The
// method of PICTURES has a result that is {image: {mimeType, data}}: a JPEG of the
// page as base64 within the size capture.js allows, and nothing else. The method
// of REPORTS has a result that is {origin, omitted, entries}: the entries of the
// page's console, each of the closed shape consoles.js says, with the page's own
// words as bounded text.

import { isPicture } from "./capture.js";
import { isWebUrl } from "./cdp.js";
import { DEFAULT_ENTRIES, DEFAULT_LEVELS, LEVELS, MAX_ENTRIES, isReport } from "./consoles.js";
import { PATH_LIMIT } from "./files.js";
import { Failure, Refusal, errorOf } from "./sentences.js";
import { TOOLS } from "./tools.js";
import {
  BUTTONS,
  KEYS,
  NAME_LIMIT,
  ROLES,
  TYPED_TEXT_LIMIT,
} from "./words.js";

// The version of the request this extension reads and of the reply it makes.
export const VERSION = 1;

// What a method that was done says it did.
export const STATUSES = Object.freeze([
  "clicked",
  "double clicked",
  "right clicked",
  "typed",
  "pressed",
  "hovered",
  "dragged",
  "chose",
]);

// A string argument that has something in it and is at most `limit` long.
const shortText = (limit) => (value) =>
  typeof value === "string" && value.trim() !== "" && value.length <= limit;

// A string argument that has something in it and is at most `limit` characters
// long, counted in code points as files.js counts a path, so that a path the relay
// and files.js accept is not refused here for the pairs of units an astral
// character takes.
const shortPath = (limit) => (value) =>
  typeof value === "string" && value.trim() !== "" && [...value].length <= limit;

// An argument that is one of a list of words.
const oneOf = (words) => (value) => words.includes(value);

// What each argument has to be.
const CHECKS = Object.freeze({
  url: (value) => typeof value === "string" && isWebUrl(value),
  role: oneOf(ROLES),
  name: shortText(NAME_LIMIT),
  onto_role: oneOf(ROLES),
  onto_name: shortText(NAME_LIMIT),
  text: shortText(TYPED_TEXT_LIMIT),
  key: oneOf(KEYS),
  button: oneOf(BUTTONS),
  count: (value) => value === 1 || value === 2,
  levels: oneOf(LEVELS),
  max_entries: (value) => Number.isSafeInteger(value) && value >= 1 && value <= MAX_ENTRIES,
  path: shortPath(PATH_LIMIT),
  size: (value) => Number.isSafeInteger(value) && value >= 0,
  file_name: shortPath(PATH_LIMIT),
});

// The methods: the arguments each needs, and the ones it takes with a default.
export const METHODS = Object.freeze({
  click_element_trusted: Object.freeze({
    required: Object.freeze(["url", "role", "name"]),
    optional: Object.freeze({ button: "left", count: 1 }),
  }),
  type_text_trusted: Object.freeze({
    required: Object.freeze(["url", "role", "name", "text"]),
    optional: Object.freeze({}),
  }),
  press_key_trusted: Object.freeze({
    required: Object.freeze(["url", "key"]),
    optional: Object.freeze({}),
  }),
  hover_element: Object.freeze({
    required: Object.freeze(["url", "role", "name"]),
    optional: Object.freeze({}),
  }),
  drag_element: Object.freeze({
    required: Object.freeze(["url", "role", "name", "onto_role", "onto_name"]),
    optional: Object.freeze({}),
  }),
  screenshot_full_page: Object.freeze({
    required: Object.freeze(["url"]),
    optional: Object.freeze({}),
  }),
  read_console: Object.freeze({
    required: Object.freeze(["url"]),
    optional: Object.freeze({ levels: DEFAULT_LEVELS, max_entries: DEFAULT_ENTRIES }),
  }),
  choose_file: Object.freeze({
    required: Object.freeze(["url", "name", "path", "size", "file_name"]),
    // The role has no default: a call that leaves it out names the file input by
    // its name alone.
    optional: Object.freeze({ role: undefined }),
  }),
});

// The methods whose result is a picture and not a status word. This table says
// which they are, and no request or result does.
export const PICTURES = Object.freeze(["screenshot_full_page"]);

// The methods whose result is the page's console and not a status word. This
// table says which they are, and no request or result does.
export const REPORTS = Object.freeze(["read_console"]);

// What each method does with the arguments it was given: the tools of tools.js.
export const HANDLERS = TOOLS;

// Whether `value` is an object that holds named members.
const isPlain = (value) =>
  typeof value === "object" && value !== null && !Array.isArray(value);

// The method and the arguments a request carries, or a Refusal. The arguments
// are those the method takes and no others, with the defaults filled in.
export function parse(message) {
  if (
    !isPlain(message) ||
    message.v !== VERSION ||
    typeof message.method !== "string" ||
    !Object.hasOwn(METHODS, message.method) ||
    Object.keys(message).some((key) => !["v", "method", "params"].includes(key))
  ) {
    throw new Refusal("request");
  }
  const { required, optional } = METHODS[message.method];
  const params = message.params ?? {};
  if (!isPlain(params)) {
    throw new Refusal("request");
  }
  const taken = [...required, ...Object.keys(optional)];
  if (Object.keys(params).some((key) => !taken.includes(key))) {
    throw new Refusal("extra");
  }
  const args = {};
  for (const name of taken) {
    const given = Object.hasOwn(params, name);
    if (!given && required.includes(name)) {
      throw new Refusal("argument", name);
    }
    const value = given ? params[name] : optional[name];
    if (!given && value === undefined) {
      continue;
    }
    if (!CHECKS[name](value)) {
      throw new Refusal("argument", name);
    }
    args[name] = value;
  }
  if (args.button === "right" && args.count !== 1) {
    throw new Refusal("argument", "count");
  }
  return { method: message.method, args: Object.freeze(args) };
}

// The reply for a method that was done: a status of STATUSES and the origin of
// an address on the web, and nothing else the method returned. For a method of
// PICTURES it is the picture, and for a method of REPORTS the console's report,
// and nothing else.
function done(method, outcome) {
  if (PICTURES.includes(method)) {
    if (!isPicture(outcome?.image)) {
      throw new Failure("failed");
    }
    const { mimeType, data } = outcome.image;
    return { v: VERSION, result: { image: { mimeType, data } } };
  }
  if (REPORTS.includes(method)) {
    if (!isReport(outcome) || !isWebUrl(outcome.origin) || new URL(outcome.origin).origin !== outcome.origin) {
      throw new Failure("failed");
    }
    const { origin, omitted, entries } = outcome;
    return { v: VERSION, result: { origin, omitted, entries: entries.map((entry) => ({ ...entry })) } };
  }
  const origin = outcome?.origin;
  if (
    !STATUSES.includes(outcome?.status) ||
    typeof origin !== "string" ||
    !isWebUrl(origin) ||
    new URL(origin).origin !== origin
  ) {
    throw new Failure("failed");
  }
  return { v: VERSION, result: { status: outcome.status, origin } };
}

// One request at a time: a second while one runs is refused and starts nothing.
let busy = false;

// The reply to a request. It never throws and never holds more than the closed
// words above. `handlers` is the table the methods run from.
export async function answer(message, chrome, handlers = HANDLERS) {
  try {
    const { method, args } = parse(message);
    if (busy) {
      throw new Refusal("busy");
    }
    busy = true;
    try {
      return done(method, await handlers[method](args, chrome));
    } finally {
      busy = false;
    }
  } catch (error) {
    return { v: VERSION, error: errorOf(error) };
  }
}

// Sends a tool of the input-trusted group to the BraveBot Trusted Input
// extension, which holds `debugger` and makes the real mouse and keyboard input
// (extension-trusted/). This extension only sends: it declares no
// externally_connectable and registers no listener for a message from another
// extension, so the other extension is the one that takes a request from this
// one, by this extension's id, and nothing takes one from the other
// (BROWSER-12, BROWSER-37).
//
// A tool calls forward after handle in tools.js has checked the tool and group
// switches, the group's lifetime and the allowed sites, and after the tool has
// checked its arguments and the blocked sites. The activity log keeps the
// group, the tool, the outcome and the origin of the page, and nothing of the
// arguments.
//
// The reply holds words this extension knows: a status word and an origin, or a
// kind word and one sentence. There are two exceptions. The tool of
// PICTURE_METHODS has a reply that is a JPEG as base64 in {image: {mimeType,
// data}}, checked for its form and size here (pictures.js) and passed on as it
// is. The tool of REPORT_METHODS has a reply that is the page's console as an
// origin and a list of entries of a closed shape, checked here (consoles.js) and
// made into text here. Which tool each is comes from these tables and from no
// reply. A reply of any other shape is a failure with a sentence written here.

import { isReport, reportText } from "./consoles.js";
import { Refusal, ToolError } from "./errors.js";
import {
  MAX_PICTURE_BYTES,
  PICTURE_TYPE,
  decodedSize,
  isBase64,
} from "./pictures.js";
import { shownParts } from "./shown.js";

// The id of the BraveBot Trusted Input extension, which the browser derives from
// the key in that extension's manifest. A copy of the extension with another id,
// such as a Chrome Web Store build, is not sent to.
export const SECOND_ID = "lmdelobjcmmjoapnmnkfpmjlfikgepbl";

// The version of the request this extension sends and of the reply it reads.
export const VERSION = 1;

// How long forward waits for the reply, in milliseconds. The person has 60
// seconds to answer the window of the other extension, and the relay waits 90
// seconds for this extension's reply (BROWSER-6), so a call that is not answered
// ends here, with room on both sides.
export const WAIT_MS = 80_000;

// What the other extension says it did, one word for each tool and click.
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

// The methods whose result is a picture and not a status word. A reply to any
// other method that holds a picture is not understood, and so is a reply to
// these that holds a status.
export const PICTURE_METHODS = Object.freeze(["screenshot_full_page"]);

// The methods whose result is the page's console and not a status word. A reply
// to any other method that holds a console is not understood, and so is a reply
// to these that holds a status.
export const REPORT_METHODS = Object.freeze(["read_console"]);

// The kinds of error the other extension replies with: it refused by one of its
// rules, or it tried and did not complete.
const REFUSAL = "refusal";
const FAILED = "failed";

// The most characters of the other extension's sentence that are passed on.
export const SENTENCE_LIMIT = 300;

// What a call is told when the browser has no extension to deliver it to. The
// browser words an extension that is not installed, one that is turned off and
// one that does not accept this sender alike, so the sentence names all three.
export const NOT_THERE =
  "the BraveBot Trusted Input extension is not installed, is turned off in Brave, or does not accept this extension, so nothing was done";

// What a call is told when the other extension's worker stopped before it
// answered.
export const STOPPED =
  "the BraveBot Trusted Input extension stopped before it answered, so the action may or may not have been done";

// What a call is told when the reply does not come in time.
export const TOO_LATE = `the BraveBot Trusted Input extension did not answer within ${WAIT_MS / 1000} seconds, so the action may or may not have been done`;

// What a call is told for a reply of a shape this extension does not know.
export const NOT_UNDERSTOOD =
  "the BraveBot Trusted Input extension answered with something this extension does not understand, so the action may or may not have been done";

// What the browser says when no extension takes a message.
const NO_RECEIVER = /Receiving end does not exist|Could not establish connection/;

// What the sentences of the other extension are given after.
const REFUSED_PREFIX = "the BraveBot Trusted Input extension refused it: ";
const FAILED_PREFIX = "the BraveBot Trusted Input extension could not do it: ";

// Whether `value` is an object that holds named members.
const isPlain = (value) =>
  typeof value === "object" && value !== null && !Array.isArray(value);

// The reply of `chrome.runtime.sendMessage`, or the error that ends the wait.
// A browser that has nothing to deliver to rejects, and so does one whose
// receiver stops before it replies, and the two are told apart by what the
// browser says.
async function sent(chrome, request) {
  let timer;
  const late = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new ToolError(TOO_LATE)), WAIT_MS);
  });
  try {
    return await Promise.race([
      chrome.runtime.sendMessage(SECOND_ID, request),
      late,
    ]);
  } catch (error) {
    if (error instanceof ToolError) {
      throw error;
    }
    if (NO_RECEIVER.test(String(error?.message))) {
      throw new Refusal(NOT_THERE);
    }
    throw new ToolError(STOPPED);
  } finally {
    clearTimeout(timer);
  }
}

// Whether `origin` is the origin of an address on the web, written as a browser
// writes one.
function isOrigin(origin) {
  try {
    const parsed = new URL(origin);
    return ["http:", "https:"].includes(parsed.protocol) && parsed.origin === origin;
  } catch {
    return false;
  }
}

// Whether `sentence` can be passed on as it is: text with no character a person
// cannot see, and no more than SENTENCE_LIMIT characters.
function isPlainSentence(sentence) {
  if (typeof sentence !== "string" || sentence === "") {
    return false;
  }
  const { pieces, more } = shownParts(sentence, SENTENCE_LIMIT);
  return more === 0 && pieces.every((piece) => typeof piece === "string");
}

// The picture a result holds, as {image: {mimeType, data}} and nothing else: a
// JPEG, its base64 in the canonical form, and at most MAX_PICTURE_BYTES once
// decoded. A picture over that is never cut.
function pictureOf(result) {
  const { image } = result;
  if (
    Object.keys(result).length !== 1 ||
    !isPlain(image) ||
    Object.keys(image).sort().join() !== "data,mimeType" ||
    image.mimeType !== PICTURE_TYPE ||
    !isBase64(image.data) ||
    decodedSize(image.data) > MAX_PICTURE_BYTES
  ) {
    throw new ToolError(NOT_UNDERSTOOD);
  }
  return { image: { mimeType: image.mimeType, data: image.data } };
}

// The text of the console a result holds: the result has to be of the closed
// shape, and its origin the origin of an address on the web. Nothing of it is
// passed on but the text that is made from it.
function consoleOf(result) {
  if (!isReport(result) || !isOrigin(result.origin)) {
    throw new ToolError(NOT_UNDERSTOOD);
  }
  return reportText(result);
}

// What a reply says, as the result of the tool or the error that ends it. Only a
// status word of STATUSES and an origin are taken from a result, or for a method
// of PICTURE_METHODS the picture, or for a method of REPORT_METHODS the text of
// its console, and from an error its kind and one plain sentence.
function outcomeOf(reply, method) {
  if (!isPlain(reply) || reply.v !== VERSION) {
    throw new ToolError(NOT_UNDERSTOOD);
  }
  const { result, error } = reply;
  if (isPlain(result) && error === undefined) {
    if (PICTURE_METHODS.includes(method)) {
      return pictureOf(result);
    }
    if (REPORT_METHODS.includes(method)) {
      return consoleOf(result);
    }
    if (!STATUSES.includes(result.status) || !isOrigin(result.origin)) {
      throw new ToolError(NOT_UNDERSTOOD);
    }
    return { status: result.status, origin: result.origin };
  }
  if (isPlain(error) && result === undefined) {
    if (![REFUSAL, FAILED].includes(error.kind) || !isPlainSentence(error.message)) {
      throw new ToolError(NOT_UNDERSTOOD);
    }
    throw error.kind === REFUSAL
      ? new Refusal(`${REFUSED_PREFIX}${error.message}`)
      : new ToolError(`${FAILED_PREFIX}${error.message}`);
  }
  throw new ToolError(NOT_UNDERSTOOD);
}

// Sends `method` with `params` to the other extension and answers what it did,
// or throws what it refused or could not do. `params` are the arguments tools.js
// checked, and no others.
export async function forward(chrome, method, params) {
  return outcomeOf(await sent(chrome, { v: VERSION, method, params }), method);
}

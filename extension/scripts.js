// The group `scripts` and its one tool, run_script: code the planner wrote, run in
// the top document of one open page after the person has read all of it in a
// window of this extension. run_script itself is in tools.js, beside the
// helpers that find the page and ask the person. This file holds what is
// specific to the browser's userScripts API: the limits, the world's policy, the
// check that the API is usable, the text the window is given and the text a
// result becomes.
//
// What limits the code is that the person read it. The world is configured with
// a content security policy of default-src 'none', which Brave 1.98.62 applied to
// fetch, XMLHttpRequest, sendBeacon, WebSocket, EventSource, images, link
// elements, script elements and import(). It did not apply to a frame's source, a
// style's image, audio and object elements, a worker, a navigation, a form
// submission, an anchor click, a meta refresh, or window.open after the page has
// had a click, and the code can read everything in the page and make the page
// click and submit as the person. Brave 1.98.63 applies the policy to the first
// ten worlds a document is given and to none after that, so each call checks it
// with a probe before the person is asked. BROWSER-46 states it.

import { ToolError } from "./errors.js";
import { TYPED_TEXT_LIMIT } from "./words.js";

// The most characters of code a call may hold. The confirmation window shows the
// code whole in the frame it uses for typed text and cannot be allowed for more
// than that, so a longer code is refused here and never cut.
export const CODE_LIMIT = TYPED_TEXT_LIMIT;

// The most UTF-16 code units of a result that come back, as read_page's text. A
// longer result is cut and says so.
export const RESULT_LIMIT = 100_000;

// The policy every script's world is configured with, before the script runs.
// Brave fixes a world's policy when the world is first made in a document, so
// each call uses a world of its own and the policy is never changed afterwards.
export const WORLD_CSP = "default-src 'none'";

// The code each call runs first in its world, before the person is asked, to find
// out whether the browser applied WORLD_CSP to it. It is text of this extension,
// reads nothing of the page, sets no variable and answers "closed" where the
// policy stopped the request and "open" where it did not. A fetch of a data:
// address is stopped by default-src 'none'. Brave 1.98.63 gave the eleventh and
// later worlds of a document no policy, and the twelfth and later one world
// between them, so the answer there was "open".
export const POLICY_PROBE = 'fetch("data:,x").then(() => "open", () => "closed")';
export const POLICY_STOPPED = "closed";

// What a call is told when the probe did not answer "closed". Nothing of the
// person's code has run.
export const POLICY_NOT_APPLIED =
  "the browser did not apply the policy that stops the script's own requests in this page, so nothing was run; reload the page and ask again";

// The name the confirmation window says the action is (actions.js).
export const SCRIPT_ACTION = "run_script";

// What a call is told while the browser has not made userScripts usable. It names
// the switch the person turns on, which this extension cannot.
export const UNAVAILABLE =
  "run_script is refused until Allow User Scripts is turned on for the BraveBot extension on its details page at brave://extensions (in a browser older than Chrome 138, until Developer mode is turned on at brave://extensions)";

// What the shape of a reference's name is, as the planner is told it. A code that
// is nothing else is a mistake for a reference to be run, which this process
// never does, and is refused before anybody is asked.
const REFERENCE_NAME = /^\s*ref:\d+\s*$/;

// Whether `code` is only the name of a reference.
export function namesAReference(code) {
  return REFERENCE_NAME.test(code);
}

// Whether the browser lets this extension use userScripts now. With the
// permission not held, or the person's switch off, the namespace is absent or each
// of its methods throws that it is not available in this context (Brave
// 1.98.62), and a call that lists the world configurations does neither of the
// things a script does.
export async function userScriptsAvailable(chrome) {
  try {
    await chrome.userScripts.getWorldConfigurations();
    return true;
  } catch {
    return false;
  }
}

// What the confirmation window is given for a script: the page, and the whole
// code. The rows of an element are empty and the window does not show them
// (actions.js says the action acts on none).
export function describeScript({ origin, url, code }) {
  return Object.freeze({
    action: SCRIPT_ACTION,
    origin,
    url,
    tag: "",
    role: null,
    name: "",
    excerpt: "",
    goes: "",
    code,
  });
}

// A name for a world no other call uses. The browser takes a name of at most 256
// characters that does not start with an underscore.
export function newWorldId() {
  return `bravebot-${globalThis.crypto.randomUUID()}`;
}

// The first `limit` UTF-16 code units of `text`, without half of a surrogate pair
// at the end.
function cutAt(text, limit) {
  const kept = text.slice(0, limit);
  const last = kept.charCodeAt(kept.length - 1);
  return last >= 0xd800 && last <= 0xdbff ? kept.slice(0, -1) : kept;
}

// What a script answered, as text: a string as it is, and anything else as JSON,
// which is nothing for a script that answered nothing. Fails, saying so and
// nothing of the value, where the value cannot be written as JSON. `cut` says the
// text is the start of a longer one.
export function resultText(value) {
  let written = value;
  if (typeof value !== "string") {
    try {
      written = JSON.stringify(value) ?? "";
    } catch {
      throw new ToolError(
        "the script answered with a value that cannot be written as JSON",
      );
    }
  }
  return written.length > RESULT_LIMIT
    ? { text: cutAt(written, RESULT_LIMIT), cut: true }
    : { text: written, cut: false };
}

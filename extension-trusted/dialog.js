// The dialog panel: the page in the small window the service worker opens when
// a page has opened a dialog (alert, confirm, prompt or a question about leaving
// the page) while the extension sent it input. It shows the dialog as text and
// sends the worker the person's answer. The dialog stays open in the page until
// the person answers here.
//
// It follows the rules of the confirmation window (confirm.js): an extension
// page that no web page can open or reach, every word of the page's put in as
// text and made visible first (shown.js), Dismiss focused when it opens, Accept
// not available until a second after the panel is in front and only for a click
// a person made, and a denial of everything else. Both buttons are answers: the
// worker is told which one and, for a prompt, the text typed, in the message
// that gives it its token. A panel that is closed, a worker that stops and no
// answer in time give no answer, and the dialog is left open.

import { ALLOW_DELAY_MS, CONFIRMATION_PORT, HEARTBEAT_MS } from "./approval.js";
import { shownParts } from "./shown.js";
import { TARGET_LIMIT, TYPED_TEXT_LIMIT, URL_LIMIT } from "./words.js";

// What each kind of dialog is, in words the panel owns.
const KINDS = Object.freeze({
  alert: "The page shows a message and waits for OK",
  confirm: "The page asks a question that is answered yes or no",
  prompt: "The page asks a question that is answered with text",
  beforeunload: "The page asks whether to leave it",
});

const element = (id) => document.getElementById(id);
const accept = element("accept");
const dismiss = element("dismiss");
const status = element("status");
const wait = element("wait");
const typed = element("answer");

// The title and the heading name the extension that asks, as its manifest does.
const asker = chrome.runtime.getManifest().name;
document.title = `${asker} asks how to answer a dialog`;
element("heading").textContent = `How should ${asker} answer this dialog?`;

let described = false;
let finished = false;
let kind = null;
let arming = null;
let ticking = null;

// Why a dialog cannot be shown, or null. A dialog the panel cannot show it
// cannot be answered.
function problemWith(description) {
  if (typeof description !== "object" || description === null) {
    return "it has no description";
  }
  if (!Object.hasOwn(KINDS, description.type)) {
    return "it is a kind of dialog this window does not know";
  }
  for (const name of ["origin", "url", "message", "defaultPrompt"]) {
    if (typeof description[name] !== "string") {
      return `it has no ${name}`;
    }
  }
  return null;
}

// Puts `text` in `target` as text, with each character that cannot be seen
// marked by its code point in an element of its own, and no more than `limit`
// characters. Nothing is parsed.
function put(target, text, limit) {
  const { pieces, more } = shownParts(text, limit);
  const nodes = pieces.map((piece) => {
    if (typeof piece === "string") {
      return piece;
    }
    const mark = document.createElement("span");
    mark.className = "mark";
    mark.textContent = piece.mark;
    return mark;
  });
  const aside = (words) => {
    const made = document.createElement("span");
    made.className = "mark";
    made.textContent = words;
    nodes.push(made);
  };
  if (more > 0) {
    aside(`${more} more characters not shown`);
  }
  if (nodes.length === 0) {
    aside("nothing");
  }
  target.replaceChildren(...nodes);
}

function say(words) {
  status.textContent = words;
}

function finish(words) {
  finished = true;
  clearTimeout(arming);
  clearInterval(ticking);
  accept.disabled = true;
  dismiss.disabled = true;
  typed.disabled = true;
  say(words);
}

// Accept is unavailable, and becomes available a second after the panel is in
// front, if it still is.
function disarm() {
  clearTimeout(arming);
  accept.disabled = true;
}

function arm() {
  disarm();
  if (finished || !described || !document.hasFocus()) {
    return;
  }
  arming = setTimeout(() => {
    accept.disabled = finished || !document.hasFocus();
  }, ALLOW_DELAY_MS);
}

const port = chrome.runtime.connect({ name: CONFIRMATION_PORT });

// Tells the worker the person's answer: allow is true for both buttons, since
// each one answers the dialog, and `accept` says which.
function answer(accepted) {
  if (finished) {
    return;
  }
  const text = accepted && kind === "prompt" ? typed.value : "";
  finish(accepted ? "Accepted. This window closes." : "Dismissed. This window closes.");
  try {
    port.postMessage({ type: "answer", allow: true, accept: accepted, text });
  } catch {
    say("BraveBot could not be told, so the dialog is still open. This window closes.");
  }
}

// What the panel says it waits for, and tells the worker it is still here, so
// that the worker is kept running until the answer.
function tick(deadline, count) {
  const left = Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
  wait.textContent =
    left > 0
      ? `The dialog stays open in the page if you do not answer in ${left} seconds.`
      : "The time is up. The dialog stays open in the page.";
  if (count > 0 && count % Math.round(HEARTBEAT_MS / 1000) === 0) {
    try {
      port.postMessage({ type: "alive" });
    } catch {
      // The disconnect handler ends the panel.
    }
  }
}

function show({ description, deadline }) {
  const problem = problemWith(description);
  if (problem !== null) {
    finish(`This dialog cannot be shown, so it cannot be answered here: ${problem}.`);
    return;
  }
  kind = description.type;
  element("kind").textContent = KINDS[kind];
  put(element("origin"), description.origin, URL_LIMIT);
  put(element("url"), description.url, URL_LIMIT);
  put(element("message"), description.message, TYPED_TEXT_LIMIT);
  element("prompt-section").hidden = kind !== "prompt";
  if (kind === "prompt") {
    put(element("suggested"), description.defaultPrompt, TARGET_LIMIT);
  }
  described = true;
  let count = 0;
  tick(deadline, count);
  ticking = setInterval(() => tick(deadline, (count += 1)), 1000);
  dismiss.focus();
  arm();
}

port.onMessage.addListener((message) => {
  if (message?.type === "show" && !described && !finished) {
    show(message);
  } else if (message?.type === "ended" && !finished) {
    finish("This request has ended. The dialog stays open in the page.");
    window.close();
  }
});

port.onDisconnect.addListener(() => {
  if (!finished) {
    finish("BraveBot is no longer waiting for this window, so the dialog stays open in the page.");
  }
  window.close();
});

dismiss.addEventListener("click", () => answer(false));

accept.addEventListener("click", (event) => {
  if (event.isTrusted !== true || finished || accept.disabled || !described) {
    return;
  }
  const gesture = globalThis.navigator?.userActivation;
  if (gesture && gesture.isActive !== true) {
    return;
  }
  answer(true);
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    answer(false);
  }
});
window.addEventListener("focus", arm);
window.addEventListener("blur", disarm);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    arm();
  } else {
    disarm();
  }
});

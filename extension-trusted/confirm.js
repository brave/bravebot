// The confirmation window: the page in the small window the service worker opens
// when a tool of the input group has found the element a call names. It shows
// what is about to be done and to which element, as text, and sends the worker
// the person's answer. Nothing is done until the person allows it, here.
//
// The window is an extension page: no web page can open it, reach it or send it
// a message. It gets what to show from the worker over a port, and every word of
// it that a web page chose goes into the window as text and is made visible
// first (shown.js). The answer is sent over the same port, and the worker
// accepts it only from this window while it waits. A window that is closed, a
// worker that stops, or no answer in time is a denial.
//
// Allow once is not available until a second after the window is shown or comes
// back to the front, and not while the window is out of front. When the box that
// holds the code, the text or the path a call carries holds more than it shows,
// it is not available until the box has been scrolled to its end. It takes only a
// click or a key the person made. Deny has the focus when the window opens, so a
// click or Enter made as the window appears under the pointer, or after a page
// took the front, does not allow.

import { ACTIONS, CARRIED } from "./actions.js";
import { ALLOW_DELAY_MS, CONFIRMATION_PORT, HEARTBEAT_MS } from "./approval.js";
import { shownParts } from "./shown.js";
import {
  EXCERPT_LIMIT,
  NAME_LIMIT,
  TARGET_LIMIT,
  TYPED_TEXT_LIMIT,
  URL_LIMIT,
} from "./words.js";

const element = (id) => document.getElementById(id);
const allow = element("allow");
const deny = element("deny");
const status = element("status");
const wait = element("wait");
const typedSection = element("typed-section");
const typedBox = element("typed-box");

// The title and the heading name the extension that asks, as its manifest does,
// so that a person who has both extensions can tell which one's window is open.
const asker = chrome.runtime.getManifest().name;
document.title = `${asker} asks before it acts`;
element("heading").textContent = `Allow ${asker} to do this in the page?`;

let described = false;
let finished = false;
let arming = null;
let ticking = null;
// Whether the second after the window was shown or came to the front has passed,
// and whether the box has been scrolled to its end. A box that has been is read
// until the window is made another size.
let armed = false;
let boxRead = false;

// Why a description cannot be shown, or null. A description the window cannot
// show in full cannot be allowed.
function problemWith(description) {
  if (typeof description !== "object" || description === null) {
    return "it has no description";
  }
  if (!Object.hasOwn(ACTIONS, description.action)) {
    return "it names an action this window does not know";
  }
  for (const name of ["origin", "url", "tag", "name", "excerpt", "goes"]) {
    if (typeof description[name] !== "string") {
      return `it has no ${name}`;
    }
  }
  if (description.role !== null && typeof description.role !== "string") {
    return "its role is not text";
  }
  if (description.goesUnknown !== undefined && description.goesUnknown !== true) {
    return "its note that it is not known where it goes is not one the window knows";
  }
  if (description.number !== undefined && !isCount(description.number)) {
    return "its number is not a whole number";
  }
  const place = description.position;
  if (
    place !== undefined &&
    (description.number === undefined ||
      !isCount(place?.index) ||
      !isCount(place?.of) ||
      place.index > place.of)
  ) {
    return "its place among the elements of its name is not whole numbers";
  }
  const carried = CARRIED[description.action];
  if (carried) {
    const value = description[carried[0]];
    if (typeof value !== "string" || value === "") {
      return `it has no ${carried[0]}`;
    }
    if ([...value].length > TYPED_TEXT_LIMIT) {
      return `its ${carried[0]} is too long to show in full`;
    }
  }
  return null;
}

// Whether `value` is a whole number from 1, which is how a number and a place
// among elements are counted.
function isCount(value) {
  return Number.isSafeInteger(value) && value >= 1;
}

// What the window says of an element that was named by its number in a list: the
// number, and which of the elements with the same role and name it is, so that
// two elements that look alike are told apart here and not only by the outline.
// It is made of numbers and words of the window's own.
function numberLine({ number, position }) {
  const place = position
    ? `, ${position.index} of ${position.of} with this role and name`
    : "";
  return `${number} in the last list${place}`;
}

// Puts `text` in `target` as text, with each character that cannot be seen
// marked by its code point in an element of its own, and no more than `limit`
// characters. Nothing is parsed. With `lines`, a line feed is written out like
// the other unseen characters and a line break follows it, so that code reads as
// the lines it has. No other character breaks a line.
function put(target, text, limit, lines = false) {
  const { pieces, more } = shownParts(text, limit);
  const nodes = pieces.flatMap((piece) => {
    if (typeof piece === "string") {
      return [piece];
    }
    const mark = document.createElement("span");
    mark.className = "mark";
    mark.textContent = piece.mark;
    return lines && piece.mark === "U+000A" ? [mark, "\n"] : [mark];
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

// What the window says of the size of a value it shows whole: its characters, and
// for code its lines, which are separated by line feeds.
function countLine(value, code) {
  const characters = `${[...value].length} characters`;
  if (!code) {
    return characters;
  }
  const lines = value.split("\n").length;
  return `${lines} ${lines === 1 ? "line" : "lines"}, ${characters}`;
}

// Puts a word of the window's own in `target`, in a box that a character of a page
// cannot imitate.
function own(target, words) {
  const made = document.createElement("span");
  made.className = "mark";
  made.textContent = words;
  target.replaceChildren(made);
}

function say(words) {
  status.textContent = words;
}

function finish(words) {
  finished = true;
  clearTimeout(arming);
  clearInterval(ticking);
  allow.disabled = true;
  deny.disabled = true;
  say(words);
}

// Whether the box has been scrolled to its end, or holds no more than it shows.
// A box that is not laid out has no end to have reached, and a call that carries
// nothing has no box.
function readBox() {
  if (typedSection.hidden) {
    return true;
  }
  const atEnd = typedBox.scrollTop + typedBox.clientHeight >= typedBox.scrollHeight - 1;
  if (typedBox.clientHeight > 0 && atEnd) {
    boxRead = true;
  }
  return boxRead;
}

const BOX_UNREAD = "Allow once is not available until the box above has been scrolled to its end.";

// Sets Allow once to what the window now allows, and says why it is not available
// while the box has not been read.
function refresh() {
  const read = readBox();
  if (!finished) {
    say(read ? "" : BOX_UNREAD);
  }
  allow.disabled = finished || !armed || !document.hasFocus() || !read;
}

// Allow once is unavailable, and becomes available a second after the window is
// in front, if it still is and the box has been read.
function disarm() {
  clearTimeout(arming);
  armed = false;
  allow.disabled = true;
}

function arm() {
  disarm();
  if (finished || !described || !document.hasFocus()) {
    return;
  }
  arming = setTimeout(() => {
    armed = true;
    refresh();
  }, ALLOW_DELAY_MS);
}

const port = chrome.runtime.connect({ name: CONFIRMATION_PORT });

function answer(allowed) {
  if (finished) {
    return;
  }
  finish(allowed ? "Allowed. This window closes." : "Denied. This window closes.");
  try {
    port.postMessage({ type: "answer", allow: allowed });
  } catch {
    say("BraveBot could not be told, so nothing was done. This window closes.");
  }
}

// What the window says it waits for, and tells the worker it is still here, so
// that the worker is kept running until the answer.
function tick(deadline, count) {
  const left = Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
  wait.textContent =
    left > 0
      ? `Nothing is done if you do not answer in ${left} seconds.`
      : "The time is up. Nothing is done.";
  if (count > 0 && count % Math.round(HEARTBEAT_MS / 1000) === 0) {
    try {
      port.postMessage({ type: "alive" });
    } catch {
      // The disconnect handler ends the window.
    }
  }
}

function show({ description, deadline }) {
  const problem = problemWith(description);
  if (problem !== null) {
    finish(`This request cannot be shown, so it cannot be allowed: ${problem}.`);
    return;
  }
  element("action").textContent = ACTIONS[description.action].words;
  // An action that acts on no element has no element to show: the block of the
  // element, its words and the note about the outline are not on the page.
  element("element-section").hidden = ACTIONS[description.action].element === false;
  const carried = CARRIED[description.action];
  element("typed-section").hidden = carried === undefined;
  if (carried) {
    // A value that is not text, such as a size, has no count of characters. Code
    // is counted in lines as well, so that a person can tell whether they have
    // seen the last one.
    const [name, heading, counted = true, code = false] = carried;
    element("typed-heading").textContent = heading;
    element("typed-section").className = code ? "code" : "";
    put(element("typed"), description[name], Infinity, code);
    element("typed-count").hidden = !counted;
    element("typed-count").textContent = counted ? countLine(description[name], code) : "";
  }
  const numbered = description.number !== undefined;
  element("number-term").hidden = !numbered;
  element("number").hidden = !numbered;
  element("number").textContent = numbered ? numberLine(description) : "";
  put(element("origin"), description.origin, URL_LIMIT);
  put(element("url"), description.url, URL_LIMIT);
  put(element("role"), description.role ?? "", NAME_LIMIT);
  put(element("tag"), description.tag, NAME_LIMIT);
  put(element("name"), description.name, NAME_LIMIT);
  put(element("excerpt"), description.excerpt, EXCERPT_LIMIT);
  if (description.goesUnknown === true) {
    own(element("goes"), "unknown");
  } else {
    put(element("goes"), description.goes, TARGET_LIMIT);
  }
  described = true;
  refresh();
  let count = 0;
  tick(deadline, count);
  ticking = setInterval(() => tick(deadline, (count += 1)), 1000);
  deny.focus();
  arm();
}

port.onMessage.addListener((message) => {
  if (message?.type === "show" && !described && !finished) {
    show(message);
  } else if (message?.type === "ended" && !finished) {
    finish("This request has ended. Nothing was done.");
    window.close();
  }
});

port.onDisconnect.addListener(() => {
  if (!finished) {
    finish("BraveBot is no longer waiting for this window, so nothing was done.");
  }
  window.close();
});

deny.addEventListener("click", () => answer(false));

allow.addEventListener("click", (event) => {
  if (event.isTrusted !== true || finished || allow.disabled || !described) {
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
typedBox.addEventListener("scroll", () => {
  if (described && !finished) {
    refresh();
  }
});
// A box of another size scrolls where it did not, or no longer does.
window.addEventListener("resize", () => {
  boxRead = false;
  if (described && !finished) {
    refresh();
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


// Page interaction: what the input group's tools do inside a page, and the
// rules that stand between a name the planner wrote and an element acted on.
//
// A tool names one tab by its exact URL, and an element by a role from ROLES and
// an accessible name. Nothing else the planner wrote reaches the page: it is
// compared with the names the page has, and typed text is given to the field as
// text, so no word of it is run as code, read as a selector or parsed as HTML
// (the test over inPage and scrollInPage holds the source to that). The page
// decides which element a name leads to, so the element is found twice: once to
// show the person, in their browser, what it is, and again at the moment of the
// action, which goes ahead only if it is the element they were shown.
//
// What comes back from the page is data the person reads and nothing else
// decides: the tool's reply holds a fixed word and a count, never a word of the
// page. The functions injected into a page run in its isolated world, so the
// page's own scripts cannot change what they call. They cannot use anything else
// of this module.
//
// The isolated world does not stop the page's markup. A form answers a property
// of its own with the control it holds that has that name, so a control named
// "action" stands in for form.action and one named "parentElement" for the
// form's parent, and the document does the same for an image or a form with a
// name. Every function injected into a page therefore reads a node through the
// prototype, with `read` and `call` as pageReads writes them, and never as
// `node.name`. The test over the injected functions holds each to that.

import { CONFIRMATION_WAIT_MS } from "./approval.js";
import { SHOWN_CAP } from "./words.js";

// The lists of words and the limits are those of words.js, which the second
// extension shares.
export {
  BUTTONS,
  DIRECTIONS,
  EXCERPT_LIMIT,
  KEYS,
  NAME_LIMIT,
  ROLES,
  SHOWN_CAP,
  TARGET_LIMIT,
  TYPED_TEXT_LIMIT,
  URL_LIMIT,
} from "./words.js";

// The most pages a scroll moves, and the longest and default wait, in seconds.
export const MAX_PAGES = 5;
export const MAX_WAIT_SECONDS = 30;
export const DEFAULT_WAIT_SECONDS = 10;

// How often wait_for_element looks again, in milliseconds.
export const WAIT_POLL_MS = 250;

// How long the outline in the page stays. It hides itself a little later than the
// confirmation window denies, for the case that the service worker is gone and
// cannot remove it.
export const OUTLINE_MS = CONFIRMATION_WAIT_MS + 10_000;

// What an element was found as, besides ok: the words inPage answers with, and
// what each says. The reply to a call holds one of these sentences, and a count
// where the sentence has one, and nothing the page wrote.
export const FAILURES = Object.freeze({
  none: () =>
    "no element has that role and that name in the top document of the page, and frames are not searched",
  several: (count) =>
    `${count} elements have that role and that name, so the name does not name one`,
  hidden: () =>
    "the only element with that role and that name is hidden or has no size",
  covered: () => "another element covers the middle of it",
  offscreen: () => "the middle of it is outside the part of the page in view",
  disabled: () => "it is disabled or read-only",
  password: () => "it is a password field, which is never acted on",
  card: () => "it is a payment card field, which is never acted on",
  file: () => "it is a file chooser, which is never acted on",
  changed: () =>
    "the element changed after it was found, so nothing was done",
  stale: () =>
    "the page no longer shows the element that has that number in the last list, so nothing was done; list the elements again",
  long: () =>
    `its name, its text or where it goes is longer than ${SHOWN_CAP} characters, so the window cannot show it whole and nothing was done`,
  left: () => "the page left the URL before it was acted on",
  unsupported: () => "it is not an element this action works on",
  no_option: () => "the select has no option with that text",
  several_options: (count) => `${count} options of the select have that text`,
});

// How a function injected into a page reads, calls and writes a property of one
// of its nodes: from the node's prototype, with the node as the receiver, so
// that a control of a form that is named like the property is never met. A
// function that is injected can use nothing of this module, so each begins with
// these three lines copied, and the test over the injected functions holds every
// one of them to this text.
export function pageReads() {
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const call = (node, name, ...args) => Reflect.apply(read(node, name), node, args);
  const put = (node, name, value) => Reflect.set(Object.getPrototypeOf(node), name, value, node);
  return { read, call, put };
}

// Whether `word` is a status inPage can answer with.
export function isFailure(word) {
  return Object.hasOwn(FAILURES, word);
}

// What a failed look at the page says, from the status and count inPage gave.
// A count that is not a whole number is not shown.
export function failureText({ status, count }) {
  return FAILURES[status](Number.isInteger(count) ? count : 0);
}

// Runs in the page, in the isolated world, so it can use nothing of this
// module. Finds the one element the request names, or the element that has
// the focus where it names none, says what it is, and with step "act" does
// the action to it. It answers a status and the facts of the one element, and
// never the page's markup.
//
//   url          the URL the tab was at when it was found; the page answers
//                "left" if it is at another
//   step         "find" says what the element is, "act" acts on it, "list"
//                says which elements can be numbered
//   role, name   what to look for; role null is the element with the focus
//   at, expect   instead of a role and a name, the place in the document of an
//                element an earlier "list" gave and the fingerprint it gave
//                for it; the page answers "stale" for any other element
//   action       "click", "double_click", "right_click", "type", "select" or
//                "key", for the step "act"
//   text         what "type" puts in the field, or the option "select" picks,
//                or the key "key" sends
//   fingerprint  what "find" answered, which "act" must find again
//   limit        for "find", the most characters of the name, the element's
//                text and where it goes that it answers with, whole: it answers
//                "long" for an element with more of any; for "list", the most
//                elements, the most characters of a listed name and of a
//                fingerprint
//   outline      for "find": how many milliseconds an outline of the element
//                stays in the page, after the element is brought into view; 0
//                for none
//
// The step "list" answers the elements that have a role and are displayed and
// not refused, in tree order, each with the role, the name cut to the limit and
// whether it was cut, where it goes whole, its place in the document and its
// fingerprint, and how many
// were left out and why. It reads no value a field holds, and a password field,
// a payment card field and a file input are not among them.
//
// The elements are those of the top document and of the shadow roots in it,
// open or closed, in tree order. A frame is not entered. An element counts as
// hidden when it, or an element above it, is not displayed, is invisible or
// transparent, is marked hidden, or is one pixel or less across. The name is
// the accessible name as the browser computes it, in the parts that matter
// here: aria-labelledby, aria-label, the labels of a control, a button's value,
// an image's alt text, the text inside the element for the roles that take
// their name from it, and then the title and a field's placeholder.
export function inPage(request) {
  if (location.href !== request.url) {
    return { status: "left" };
  }
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const call = (node, name, ...args) => Reflect.apply(read(node, name), node, args);
  const put = (node, name, value) => Reflect.set(Object.getPrototypeOf(node), name, value, node);
  const ROLES = [
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "menuitem",
    "tab",
    "option",
    "switch",
  ];
  const FROM_CONTENT = [
    "button",
    "link",
    "menuitem",
    "tab",
    "option",
    "checkbox",
    "radio",
    "switch",
  ];
  const BUTTON_TYPES = ["button", "submit", "reset", "image", "file"];
  const TEXT_TYPES = ["", "text", "email", "tel", "url", "search", "password"];
  const KEY_CODES = {
    Enter: 13,
    Escape: 27,
    Tab: 9,
    ArrowLeft: 37,
    ArrowUp: 38,
    ArrowRight: 39,
    ArrowDown: 40,
    Home: 36,
    End: 35,
    PageUp: 33,
    PageDown: 34,
  };
  const shorten = (value, limit) =>
    value.length > limit ? value.slice(0, limit) : value;
  const squash = (value) => String(value ?? "").replace(/\s+/g, " ").trim();
  const attributeOf = (node, name) => call(node, "getAttribute", name);
  const tagOf = (node) => read(node, "localName");
  const shadowOf = (element) =>
    globalThis.chrome?.dom?.openOrClosedShadowRoot(element) ??
    read(element, "shadowRoot") ??
    null;
  const parentOf = (node) =>
    read(node, "parentElement") ?? read(call(node, "getRootNode"), "host") ?? null;

  // Every element in tree order with where it is: the child numbers from the
  // document down, a "/" where a shadow root is entered.
  const entries = [];
  const pending = [];
  const queue = (parent, prefix, separator) => {
    const children = Array.from(read(parent, "children") ?? []);
    for (let at = children.length - 1; at >= 0; at -= 1) {
      pending.push({ element: children[at], path: `${prefix}${separator}${at}` });
    }
  };
  queue(document, "", "");
  while (pending.length > 0) {
    const entry = pending.pop();
    entries.push(entry);
    const shadow = shadowOf(entry.element);
    if (shadow) {
      queue(shadow, entry.path, "/");
    }
    queue(entry.element, entry.path, ".");
  }

  const hiddenByAttribute = (element) =>
    read(element, "hidden") === true || attributeOf(element, "aria-hidden") === "true";
  const contentOf = (element) => {
    let pieces = "";
    for (const child of Array.from(read(element, "childNodes") ?? [])) {
      const kind = read(child, "nodeType");
      if (kind === 3) {
        pieces += read(child, "data");
      } else if (kind === 1) {
        if (hiddenByAttribute(child)) {
          continue;
        }
        const style = getComputedStyle(child);
        if (style.display === "none") {
          continue;
        }
        const label = squash(attributeOf(child, "aria-label"));
        const piece =
          label ||
          (tagOf(child) === "img"
            ? squash(attributeOf(child, "alt"))
            : contentOf(child));
        pieces += style.display === "inline" ? piece : ` ${piece} `;
      }
    }
    return squash(pieces);
  };
  const referencedName = (node) =>
    squash(attributeOf(node, "aria-label")) || contentOf(node);
  const nameOf = (element, role) => {
    const root = call(element, "getRootNode");
    const labelledby = squash(attributeOf(element, "aria-labelledby"));
    if (labelledby !== "") {
      const named = squash(
        labelledby
          .split(" ")
          .map((id) => call(root, "getElementById", id))
          .filter(Boolean)
          .map(referencedName)
          .join(" "),
      );
      if (named !== "") {
        return named;
      }
    }
    const label = squash(attributeOf(element, "aria-label"));
    if (label !== "") {
      return label;
    }
    const type = (attributeOf(element, "type") ?? "").toLowerCase();
    const labels = Array.from(read(element, "labels") ?? [])
      .map((made) => contentOf(made))
      .join(" ");
    if (squash(labels) !== "") {
      return squash(labels);
    }
    if (tagOf(element) === "input" && ["button", "submit", "reset"].includes(type)) {
      const value = squash(attributeOf(element, "value"));
      if (value !== "") {
        return value;
      }
    }
    if (tagOf(element) === "input" && type === "image") {
      const alt = squash(attributeOf(element, "alt"));
      if (alt !== "") {
        return alt;
      }
    }
    if (tagOf(element) === "option") {
      const own = squash(attributeOf(element, "label"));
      if (own !== "") {
        return own;
      }
    }
    if (FROM_CONTENT.includes(role)) {
      const content = contentOf(element);
      if (content !== "") {
        return content;
      }
    }
    const title = squash(attributeOf(element, "title"));
    if (title !== "") {
      return title;
    }
    return squash(attributeOf(element, "placeholder"));
  };

  const roleOf = (element) => {
    const given = squash(attributeOf(element, "role")).toLowerCase().split(" ");
    const explicit = given.find((word) => ROLES.includes(word));
    if (explicit) {
      return explicit;
    }
    const tag = tagOf(element);
    const type = (attributeOf(element, "type") ?? "").toLowerCase();
    if (tag === "a" || tag === "area") {
      return call(element, "hasAttribute", "href") ? "link" : null;
    }
    if (tag === "button") {
      return "button";
    }
    if (tag === "select") {
      return read(element, "multiple") || Number(attributeOf(element, "size")) > 1
        ? null
        : "combobox";
    }
    if (tag === "textarea") {
      return "textbox";
    }
    if (tag === "option") {
      return "option";
    }
    if (tag === "input") {
      if (BUTTON_TYPES.includes(type)) {
        return "button";
      }
      if (type === "checkbox" || type === "radio") {
        return type;
      }
      if (TEXT_TYPES.includes(type)) {
        return call(element, "hasAttribute", "list") ? "combobox" : "textbox";
      }
      return null;
    }
    const editable = attributeOf(element, "contenteditable");
    return editable === "" || editable === "true" || editable === "plaintext-only"
      ? "textbox"
      : null;
  };

  // The tree is as deep as the page makes it, and a loop up it ends where the
  // tree does. A page that made a node its own ancestor would otherwise hold the
  // loop for ever, so no more steps are taken than there are nodes.
  const isHidden = (element) => {
    let steps = entries.length + 1;
    for (let node = element; node && steps > 0; node = parentOf(node)) {
      steps -= 1;
      if (hiddenByAttribute(node)) {
        return true;
      }
      const style = getComputedStyle(node);
      if (style.display === "none" || style.opacity === "0") {
        return true;
      }
      if (
        node === element &&
        (style.visibility === "hidden" || style.visibility === "collapse")
      ) {
        return true;
      }
    }
    const box = call(element, "getBoundingClientRect");
    return box.width <= 1 || box.height <= 1;
  };

  // Why an element is never acted on, or null.
  const refusalOf = (element) => {
    const type = (attributeOf(element, "type") ?? "").toLowerCase();
    const complete = (attributeOf(element, "autocomplete") ?? "")
      .toLowerCase()
      .split(/\s+/);
    if (
      tagOf(element) === "input" &&
      (type === "password" ||
        complete.includes("current-password") ||
        complete.includes("new-password"))
    ) {
      return "password";
    }
    if (complete.some((word) => word.startsWith("cc-"))) {
      return "card";
    }
    if (tagOf(element) === "input" && type === "file") {
      return "file";
    }
    let disabled = read(element, "disabled") === true || read(element, "readOnly") === true;
    disabled =
      disabled || attributeOf(element, "aria-disabled") === "true";
    let steps = entries.length + 1;
    for (let node = parentOf(element); node && !disabled && steps > 0; node = parentOf(node)) {
      steps -= 1;
      disabled = tagOf(node) === "fieldset" && read(node, "disabled") === true;
    }
    return disabled ? "disabled" : null;
  };

  // Where acting on an element can lead. `goes` is an address, or "" where it leads
  // nowhere, and `unknown` is true where it cannot be told from the page.
  const NOWHERE = { goes: "", unknown: false };
  const UNKNOWN = { goes: "", unknown: true };
  // A link's address made whole. A link in a drawing has it in an object.
  const addressOf = (node) => {
    const value = read(node, "href");
    if (typeof value === "string") {
      return value;
    }
    const written = value ? String(read(value, "baseVal") ?? "") : "";
    try {
      // The address is shown to the person, not used to limit an origin.
      // nosemgrep: url-constructor-base
      return new URL(written, read(document, "baseURI")).href;
    } catch {
      return written;
    }
  };
  // Whether a click on the node submits its form: a button of the types that
  // submit, and an input of type submit or image.
  const submits = (node) => {
    const tag = tagOf(node);
    const type = (attributeOf(node, "type") ?? "").toLowerCase();
    if (tag === "button") {
      return type !== "button" && type !== "reset";
    }
    return tag === "input" && (type === "submit" || type === "image");
  };
  // The address a form is sent to when the button submits it: the button's own
  // formaction takes the place of the form's action. Null where it has no form.
  const sentBy = (button) => {
    const form = read(button, "form");
    if (!form) {
      return null;
    }
    return String(
      call(button, "hasAttribute", "formaction") ? read(button, "formAction") : read(form, "action"),
    );
  };
  // Where a click goes that is sent to the node, which then goes up through every
  // element above it, as an event does: the first link with an address, or the first
  // button that submits a form, decides it. An image map divides its image among
  // links the page does not place, so an element with one cannot be told.
  const clickGoes = (start) => {
    let steps = entries.length + 1;
    for (let node = start; node && steps > 0; node = parentOf(node)) {
      steps -= 1;
      const tag = tagOf(node);
      if ((tag === "a" || tag === "area") && call(node, "hasAttribute", "href")) {
        return { goes: addressOf(node), unknown: false };
      }
      const sent = submits(node) ? sentBy(node) : null;
      if (sent !== null) {
        return { goes: sent, unknown: false };
      }
      if ((tag === "img" || tag === "object") && call(node, "hasAttribute", "usemap")) {
        return UNKNOWN;
      }
    }
    return NOWHERE;
  };
  // Where the Enter key goes in an input that is not a button: its form, which the
  // first submit button of the form sends, with its own formaction in place of the
  // form's action, or the form's action where it has no submit button.
  const enterGoes = (element) => {
    const type = (attributeOf(element, "type") ?? "").toLowerCase();
    if (tagOf(element) !== "input" || ["button", "reset", "submit", "image", "hidden"].includes(type)) {
      return NOWHERE;
    }
    const form = read(element, "form");
    if (!form) {
      return NOWHERE;
    }
    const button = Array.from(read(form, "elements") ?? []).find(submits);
    return { goes: String(button ? sentBy(button) : read(form, "action")), unknown: false };
  };
  // Where a click sent to the element goes, and where Enter goes if it is a field.
  // Two places that differ cannot be told apart.
  const goesOf = (element) => {
    const click = clickGoes(element);
    const enter = enterGoes(element);
    if (click.unknown || enter.unknown) {
      return UNKNOWN;
    }
    if (click.goes !== "" && enter.goes !== "" && click.goes !== enter.goes) {
      return UNKNOWN;
    }
    return click.goes !== "" ? click : enter;
  };
  const fingerprintOf = (element, given, called, path, where) =>
    `${tagOf(element)}|${given}|${called}|${path}|${where.goes}${where.unknown ? "|unknown" : ""}`;

  if (request.step === "list") {
    // The elements a number can be given to. An element that is not displayed,
    // or that is never acted on, is counted and left out, so no number leads to
    // one; neither is a value a field holds read. One whose fingerprint is
    // longer than the limit cannot be stored to be compared, and is left out;
    // the fingerprint holds where the element goes whole, so what is listed has
    // a destination of that length at most, and nothing is cut from it.
    const listed = [];
    const left = { hidden: 0, refused: 0, long: 0, more: 0 };
    for (const entry of entries) {
      const given = roleOf(entry.element);
      if (given === null) {
        continue;
      }
      if (listed.length >= request.limit.listed) {
        left.more += 1;
      } else if (isHidden(entry.element)) {
        left.hidden += 1;
      } else if (refusalOf(entry.element) !== null) {
        left.refused += 1;
      } else {
        const called = nameOf(entry.element, given);
        const where = goesOf(entry.element);
        const fingerprint = fingerprintOf(entry.element, given, called, entry.path, where);
        if (fingerprint.length > request.limit.fingerprint) {
          left.long += 1;
        } else {
          listed.push({
            path: entry.path,
            fingerprint,
            role: given,
            name: shorten(called, request.limit.listName),
            cut: called.length > request.limit.listName,
            goes: where.goes,
            unknown: where.unknown,
          });
        }
      }
    }
    return { status: "ok", listed, left };
  }

  // The element the request names, or why there is none.
  let found;
  let tag;
  let role = request.role;
  let name = "";
  let position;
  if (request.role === null) {
    let active = read(document, "activeElement");
    for (;;) {
      const root = active ? shadowOf(active) : null;
      const next = root ? read(root, "activeElement") : null;
      if (!next) {
        break;
      }
      active = next;
    }
    const element = active ?? read(document, "body") ?? read(document, "documentElement");
    found = entries.find((entry) => entry.element === element) ?? {
      element,
      path: "",
    };
    role = roleOf(element);
    name = role === null ? "" : nameOf(element, role);
  } else if (request.at !== undefined) {
    // The element an earlier list gave, found by where it is in the document.
    // It is the element only if it is displayed and has the fingerprint the
    // list recorded, which is checked below.
    const entry = entries.find((other) => other.path === request.at);
    const given = entry ? roleOf(entry.element) : null;
    if (given === null || isHidden(entry.element)) {
      return { status: "stale" };
    }
    found = entry;
    role = given;
    name = nameOf(entry.element, given);
    // Which of the displayed elements with this role and name, in document
    // order, this one is, since a list can number two that look alike and the
    // window tells them apart by it.
    const peers = entries.filter(
      (other) =>
        roleOf(other.element) === given &&
        nameOf(other.element, given).toLowerCase() === name.toLowerCase() &&
        !isHidden(other.element),
    );
    position = { index: peers.indexOf(entry) + 1, of: peers.length };
  } else {
    const wanted = squash(request.name).toLowerCase();
    const named = [];
    for (const entry of entries) {
      if (roleOf(entry.element) !== request.role) {
        continue;
      }
      const given = nameOf(entry.element, request.role);
      if (given.toLowerCase() === wanted) {
        named.push({ ...entry, name: given });
      }
    }
    const shown = named.filter((entry) => !isHidden(entry.element));
    if (shown.length > 1) {
      return { status: "several", count: shown.length };
    }
    if (shown.length === 0) {
      return named.length > 0
        ? { status: "hidden", count: named.length }
        : { status: "none", count: 0 };
    }
    found = shown[0];
    name = found.name;
  }
  const element = found.element;
  tag = tagOf(element);
  const where = goesOf(element);
  const fingerprint = fingerprintOf(element, role, name, found.path, where);
  if (
    (request.at !== undefined || request.expect !== undefined) &&
    request.expect !== fingerprint
  ) {
    return { status: "stale" };
  }
  const refusal = refusalOf(element);
  if (refusal !== null) {
    return { status: refusal };
  }

  if (request.step === "find") {
    // What is told of the element is whole up to a cap, and longer than that is
    // not told at all: the window cuts each word to its own limit and says how
    // much it left out, which it can do only for what it was given whole.
    const excerpt = squash(read(element, "textContent"));
    if (
      name.length > request.limit.name ||
      excerpt.length > request.limit.excerpt ||
      where.goes.length > request.limit.target
    ) {
      return { status: "long" };
    }
    // The element is brought into view and outlined, for the person to see,
    // before the window asks. The outline is an element of ours in a closed
    // shadow root: it holds no word of the page, takes no pointer event, and is
    // removed by clearOutlineInPage. It hides itself after request.outline
    // milliseconds in case nothing removes it. The page can hide it or draw one
    // of its own, so the window carries the facts and the outline is a hint.
    if (request.outline > 0) {
      call(element, "scrollIntoView", { block: "center", inline: "center", behavior: "instant" });
    }
    const box = call(element, "getBoundingClientRect");
    if (request.outline > 0 && box.width > 0 && box.height > 0) {
      if (Object.hasOwn(globalThis, "bravebotOutline") && globalThis.bravebotOutline) {
        call(globalThis.bravebotOutline, "remove");
      }
      const host = call(document, "createElement", "div");
      const root = call(host, "attachShadow", { mode: "closed" });
      const frame = call(document, "createElement", "div");
      const px = (value) => `${Math.round(value)}px`;
      read(host, "style").cssText =
        "all: initial !important; position: absolute !important; left: 0 !important; top: 0 !important; width: 0 !important; height: 0 !important; z-index: 2147483647 !important; pointer-events: none !important;";
      read(frame, "style").cssText = `position: absolute; left: ${px(box.left + scrollX - 4)}; top: ${px(box.top + scrollY - 4)}; width: ${px(box.width + 8)}; height: ${px(box.height + 8)}; box-sizing: border-box; border: 3px solid #fb542b; border-radius: 4px; box-shadow: 0 0 0 2px #fff, 0 0 12px 4px rgba(0, 0, 0, 0.45); pointer-events: none;`;
      call(root, "append", frame);
      call(read(document, "documentElement"), "append", host);
      call(frame, "animate", [{ opacity: 1 }, { opacity: 0 }], {
        delay: request.outline,
        duration: 1,
        fill: "forwards",
      });
      globalThis.bravebotOutline = host;
    }
    return {
      status: "ok",
      fingerprint,
      tag,
      role,
      name,
      excerpt,
      goes: where.goes,
      ...(where.unknown ? { goesUnknown: true } : {}),
      ...(position === undefined ? {} : { position }),
      rect: {
        x: Math.round(box.left),
        y: Math.round(box.top),
        width: Math.round(box.width),
        height: Math.round(box.height),
      },
    };
  }

  if (request.fingerprint !== fingerprint) {
    return { status: "changed" };
  }
  const action = request.action;
  if (action === "key") {
    const key = request.text;
    const base = {
      key,
      code: key,
      keyCode: KEY_CODES[key],
      which: KEY_CODES[key],
      bubbles: true,
      cancelable: true,
      composed: true,
    };
    call(element, "dispatchEvent", new KeyboardEvent("keydown", base));
    if (key === "Enter") {
      call(element, "dispatchEvent", new KeyboardEvent("keypress", base));
    }
    call(element, "dispatchEvent", new KeyboardEvent("keyup", base));
    return { status: "done" };
  }

  call(element, "scrollIntoView", { block: "center", inline: "center", behavior: "instant" });
  const box = call(element, "getBoundingClientRect");
  const x = box.left + box.width / 2;
  const y = box.top + box.height / 2;
  if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) {
    return { status: "offscreen" };
  }
  const tree = call(element, "getRootNode");
  const top = call(read(tree, "elementFromPoint") ? tree : document, "elementFromPoint", x, y);
  if (!top) {
    return { status: "offscreen" };
  }
  const labels = Array.from(read(element, "labels") ?? []);
  const reaches =
    top === element ||
    call(element, "contains", top) ||
    labels.some((made) => made === top || call(made, "contains", top));
  if (!reaches) {
    return { status: "covered" };
  }

  if (action === "type") {
    const editable = attributeOf(element, "contenteditable");
    const isField =
      tagOf(element) === "textarea" ||
      (tagOf(element) === "input" && TEXT_TYPES.includes((attributeOf(element, "type") ?? "").toLowerCase()));
    if (isField) {
      call(element, "focus");
      const setter = Object.getOwnPropertyDescriptor(
        Object.getPrototypeOf(element),
        "value",
      )?.set;
      if (!setter) {
        return { status: "unsupported" };
      }
      setter.call(element, request.text);
      call(
        element,
        "dispatchEvent",
        new InputEvent("input", {
          bubbles: true,
          composed: true,
          inputType: "insertText",
          data: request.text,
        }),
      );
      call(element, "dispatchEvent", new Event("change", { bubbles: true }));
      return { status: "done" };
    }
    if (editable === "" || editable === "true" || editable === "plaintext-only") {
      call(element, "focus");
      call(document, "execCommand", "selectAll", false);
      return call(document, "execCommand", "insertText", false, request.text)
        ? { status: "done" }
        : { status: "unsupported" };
    }
    return { status: "unsupported" };
  }

  if (action === "select") {
    if (tagOf(element) !== "select") {
      return { status: "unsupported" };
    }
    const wanted = squash(request.text).toLowerCase();
    const options = Array.from(read(element, "options") ?? []).filter(
      (option) =>
        squash(attributeOf(option, "label") ?? read(option, "textContent")).toLowerCase() === wanted,
    );
    if (options.length !== 1) {
      return options.length === 0
        ? { status: "no_option" }
        : { status: "several_options", count: options.length };
    }
    if (read(options[0], "disabled") === true) {
      return { status: "disabled" };
    }
    call(element, "focus");
    put(options[0], "selected", true);
    call(element, "dispatchEvent", new Event("input", { bubbles: true, composed: true }));
    call(element, "dispatchEvent", new Event("change", { bubbles: true }));
    return { status: "done" };
  }

  const point = {
    clientX: x,
    clientY: y,
    bubbles: true,
    cancelable: true,
    composed: true,
  };
  if (action === "right_click") {
    call(element, "dispatchEvent", new MouseEvent("contextmenu", { ...point, button: 2, buttons: 2 }));
    return { status: "done" };
  }
  const times = action === "double_click" ? 2 : 1;
  for (let count = 1; count <= times; count += 1) {
    const down = { ...point, button: 0, buttons: 1, detail: count };
    const up = { ...point, button: 0, buttons: 0, detail: count };
    call(element, "dispatchEvent", new PointerEvent("pointerdown", { ...down, pointerType: "mouse" }));
    call(element, "dispatchEvent", new MouseEvent("mousedown", down));
    call(element, "focus");
    call(element, "dispatchEvent", new PointerEvent("pointerup", { ...up, pointerType: "mouse" }));
    call(element, "dispatchEvent", new MouseEvent("mouseup", up));
    call(element, "dispatchEvent", new MouseEvent("click", up));
  }
  if (action === "double_click") {
    call(element, "dispatchEvent", new MouseEvent("dblclick", { ...point, button: 0, detail: 2 }));
  }
  return { status: "done" };
}

// Runs in the page, as inPage does. Scrolls the top document by pages of the
// window's height, or to its top or bottom, and answers a status.
export function scrollInPage(request) {
  if (location.href !== request.url) {
    return { status: "left" };
  }
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const page = innerHeight;
  if (request.direction === "top") {
    scrollTo({ top: 0, behavior: "instant" });
  } else if (request.direction === "bottom") {
    scrollTo({ top: read(read(document, "documentElement"), "scrollHeight"), behavior: "instant" });
  } else {
    const sign = request.direction === "up" ? -1 : 1;
    scrollBy({ top: sign * request.pages * page, behavior: "instant" });
  }
  return { status: "done" };
}

// Runs in the page, as inPage does. Removes the outline inPage drew, if the
// page still has it. The page cannot reach the outline to keep it, and a page
// that removed it has nothing for this to remove.
export function clearOutlineInPage() {
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const call = (node, name, ...args) => Reflect.apply(read(node, name), node, args);
  if (Object.hasOwn(globalThis, "bravebotOutline")) {
    if (globalThis.bravebotOutline) {
      call(globalThis.bravebotOutline, "remove");
    }
    globalThis.bravebotOutline = null;
  }
  return { status: "done" };
}

// Finding the one element a role and an accessible name lead to, for a session
// on one tab. The role and the name are the browser's own: the accessibility
// tree says what each element is called and what it is, the flattened document
// says what attributes it has and where it is in the page, and the page is
// never run. Everything is read with the methods cdp.js lists as reads, so a
// resolution sends no input and needs no token.
//
// A resolution is data for the window that asks the person: text and numbers,
// some of them written by the page. It is for that window and for the action
// the person allows, and no word of it is in a reply (requests.js). The element
// is the one a role and a name lead to (resolveElement) or the one that has the
// focus (resolveFocused). requireReach asks again, just before an input, whether
// a pointer at the point would still reach the element.
//
// How it differs from the matcher of the BraveBot extension (input.js) is in
// the design document, "What the second extension's matcher is".

import { command } from "./cdp.js";
import { Failure, Refusal } from "./sentences.js";
import { ROLES, SHOWN_CAP } from "./words.js";

// The roles of the browser's accessibility tree that each role of ROLES is
// asked for as. A search field is a searchbox there and counts as a textbox, as
// it does for the BraveBot extension.
export const AX_ROLES = Object.freeze({
  button: Object.freeze(["button"]),
  link: Object.freeze(["link"]),
  textbox: Object.freeze(["textbox", "searchbox"]),
  checkbox: Object.freeze(["checkbox"]),
  radio: Object.freeze(["radio"]),
  combobox: Object.freeze(["combobox"]),
  menuitem: Object.freeze(["menuitem"]),
  tab: Object.freeze(["tab"]),
  option: Object.freeze(["option"]),
  switch: Object.freeze(["switch"]),
});

// The most nodes of the accessibility tree and of the document that are looked
// through, and the most elements with the role and the name that are looked at.
// A page past either is not searched, since the work grows with them: a page of
// 45,000 elements took 1.5 seconds and 34 MB for its accessibility tree.
export const MAX_NODES = 100_000;
export const MAX_CANDIDATES = 50;

const ELEMENT = 1;
const DOCUMENT = 9;
const FRAGMENT = 11;
const STYLES = ["display", "opacity", "visibility"];

// A page's words with white space collapsed, the form names are compared in.
const squash = (value) => String(value ?? "").replace(/\s+/g, " ").trim();

// A property of an accessibility node, such as disabled.
const property = (node, name) =>
  node.properties?.find((entry) => entry.name === name)?.value?.value;

// The document as entries by node and by backend node, each holding its
// parent. Shadow roots and the documents of frames are nested inside the
// element that holds them in the flattened document, and their children name
// them as parent, so they are entered here with that element as their parent.
function indexOf(nodes) {
  const byNode = new Map();
  const byBackend = new Map();
  const add = (node, parent) => {
    const entry = {
      node: node.nodeId,
      backend: node.backendNodeId,
      parent: node.parentId ?? parent,
      type: node.nodeType,
      tag: node.localName ?? "",
      attributes: node.attributes ?? [],
    };
    byNode.set(entry.node, entry);
    byBackend.set(entry.backend, entry);
  };
  for (const node of nodes) {
    add(node);
    for (const root of node.shadowRoots ?? []) {
      add(root, node.nodeId);
    }
    if (node.contentDocument) {
      add(node.contentDocument, node.nodeId);
    }
  }
  const top = nodes.find((node) => node.nodeType === 9);
  return {
    byNode,
    byBackend,
    base: top?.baseURL ?? "",
    address: top?.documentURL ?? "",
    top: top?.backendNodeId,
    styles: new Map(),
  };
}

const parentOf = (page, entry) => page.byNode.get(entry.parent);

// The value of an attribute of an entry, or null.
function attribute(entry, name) {
  for (let at = 0; at < entry.attributes.length; at += 2) {
    if (entry.attributes[at] === name) {
      return entry.attributes[at + 1];
    }
  }
  return null;
}

// What the browser answers to a read, or null when it says it cannot, as it
// does for a node that has no box. A rule of this extension that stops the
// read is not swallowed.
async function tryRead(session, method, params) {
  try {
    return await command(session, method, params);
  } catch (error) {
    if (error instanceof Refusal || error instanceof Failure) {
      throw error;
    }
    return null;
  }
}

// The display, opacity and visibility the browser computed for an element.
async function styleOf(session, page, entry) {
  if (!page.styles.has(entry.node)) {
    const { computedStyle } = await command(session, "CSS.getComputedStyleForNode", {
      nodeId: entry.node,
    });
    const wanted = {};
    for (const { name, value } of computedStyle) {
      if (STYLES.includes(name)) {
        wanted[name] = value;
      }
    }
    page.styles.set(entry.node, wanted);
  }
  return page.styles.get(entry.node);
}

// Whether an element is shown: the accessibility tree keeps it, nothing above
// it is marked hidden, it has a box wider and taller than one pixel, it is
// displayed and visible, and neither it nor anything above it is transparent.
async function isDisplayed(session, page, node) {
  const entry = page.byBackend.get(node.backendDOMNodeId);
  if (node.ignored || entry === undefined) {
    return false;
  }
  for (let up = entry; up; up = parentOf(page, up)) {
    if (
      up.type === ELEMENT &&
      (attribute(up, "hidden") !== null || attribute(up, "aria-hidden") === "true")
    ) {
      return false;
    }
  }
  const box = await tryRead(session, "DOM.getBoxModel", { backendNodeId: entry.backend });
  if (box === null || !(box.model.width > 1) || !(box.model.height > 1)) {
    return false;
  }
  const own = await styleOf(session, page, entry);
  if (own.visibility === "hidden" || own.visibility === "collapse") {
    return false;
  }
  for (let up = entry; up; up = parentOf(page, up)) {
    if (up.type !== ELEMENT) {
      continue;
    }
    const style = await styleOf(session, page, up);
    if (style.display === "none" || !(Number(style.opacity) > 0)) {
      return false;
    }
  }
  return true;
}

// Whether an entry is a file input. The role of a file input is button, so it is
// the attribute that says.
const isFileInput = (entry) =>
  entry.tag === "input" && (attribute(entry, "type") ?? "").toLowerCase() === "file";

// Why an element is never acted on, as the code of a Refusal or a Failure, or
// null. A password or a payment card field and a file chooser are refused. With
// `allowFile`, which only resolveFileInput gives, a file input is the one kind of
// element that is not refused for being a file chooser, and any other element is
// refused for not being one; the other refusals apply to it as to every element.
function refusalOf(entry, node, allowFile = false) {
  const type = (attribute(entry, "type") ?? "").toLowerCase();
  const complete = (attribute(entry, "autocomplete") ?? "").toLowerCase().split(/\s+/);
  if (
    entry.tag === "input" &&
    (type === "password" ||
      complete.includes("current-password") ||
      complete.includes("new-password"))
  ) {
    return new Refusal("password");
  }
  if (complete.some((word) => word.startsWith("cc-"))) {
    return new Refusal("card");
  }
  if (allowFile ? !isFileInput(entry) : isFileInput(entry)) {
    return new Refusal(allowFile ? "notfile" : "file");
  }
  if (property(node, "disabled") === true || property(node, "readonly") === true) {
    return new Failure("disabled");
  }
  return null;
}

// The form a control belongs to, or null: the form its `form` attribute names, or
// the nearest form above it in the same tree, which a shadow root ends.
function formOf(page, entry) {
  const named = attribute(entry, "form");
  if (named !== null) {
    for (const other of page.byNode.values()) {
      if (other.tag === "form" && attribute(other, "id") === named) {
        return other;
      }
    }
    return null;
  }
  for (let up = parentOf(page, entry); up && up.type === ELEMENT; up = parentOf(page, up)) {
    if (up.tag === "form") {
      return up;
    }
  }
  return null;
}

// Where a click goes, or where the Enter key goes, for the places that decide it.
// `goes` is an address, or "" where nothing is sent anywhere, and `unknown` is
// true where it cannot be told from the document.
const NOWHERE = Object.freeze({ goes: "", unknown: false });
const UNKNOWN = Object.freeze({ goes: "", unknown: true });

// A page's address made whole against the page, or as written where it cannot be.
function resolvedIn(page, value) {
  try {
    // Ordinary resolution of an address the page wrote, for the person to be shown where a click
    // goes. It is not an origin check and nothing is allowed or refused by it, so an absolute
    // address replacing the base is the result wanted.
    // nosemgrep: url-constructor-base
    return new URL(value, page.base).href;
  } catch {
    return value;
  }
}

// Whether a click on the element submits its form: a button of the types that
// submit, which are all but button and reset, and an input of type submit or image.
function isSubmitButton(entry) {
  const type = (attribute(entry, "type") ?? "").toLowerCase();
  if (entry.tag === "button") {
    return type !== "button" && type !== "reset";
  }
  return entry.tag === "input" && (type === "submit" || type === "image");
}

// The address a form is sent to when `button`, one of its submit buttons, or
// nothing, submits it: the button's own formaction takes the place of the form's
// action, and an empty one is the page's own address. Null where the button has no
// form.
function submissionOf(page, button) {
  const form = formOf(page, button);
  if (form === null) {
    return null;
  }
  const action = attribute(button, "formaction") ?? attribute(form, "action");
  return action === null || action === "" ? page.address : resolvedIn(page, action);
}

// Where a click goes that starts at `start`, an element, and then goes up through
// every element above it, as an event does: the first link with an address, or the
// first button that submits a form, decides it. An image map divides its image
// among links the document does not place, so an element with one cannot be told.
function clickGoes(page, start) {
  for (let up = start; up; up = parentOf(page, up)) {
    if (up.type !== ELEMENT) {
      continue;
    }
    if (up.tag === "a" || up.tag === "area") {
      const href = attribute(up, "href") ?? attribute(up, "xlink:href");
      if (href !== null) {
        return { goes: resolvedIn(page, href), unknown: false };
      }
    }
    if (isSubmitButton(up)) {
      const sent = submissionOf(page, up);
      if (sent !== null) {
        return { goes: sent, unknown: false };
      }
    }
    if ((up.tag === "img" || up.tag === "object") && attribute(up, "usemap") !== null) {
      return UNKNOWN;
    }
  }
  return NOWHERE;
}

// Where the Enter key goes in an input that is not a button: the form it belongs to
// is submitted by its default button, the first submit button in the form, whose
// formaction takes the place of the form's action. A form with no submit button is
// sent to its action.
function enterGoes(page, entry) {
  const type = (attribute(entry, "type") ?? "").toLowerCase();
  if (entry.tag !== "input" || ["button", "reset", "submit", "image", "hidden"].includes(type)) {
    return NOWHERE;
  }
  const form = formOf(page, entry);
  if (form === null) {
    return NOWHERE;
  }
  const button = [...page.byNode.values()].find(
    (other) => isSubmitButton(other) && formOf(page, other) === form,
  );
  if (button !== undefined) {
    return { goes: submissionOf(page, button), unknown: false };
  }
  const action = attribute(form, "action");
  return { goes: action === null || action === "" ? page.address : resolvedIn(page, action), unknown: false };
}

// Where acting on `entry` can lead, for the person to be shown: where a click that
// starts at `start` goes, and where Enter goes if the element is a field. `start` is
// the element a pointer at its middle reaches, which is the element or inside it, or
// the element itself for a key. Two places that differ cannot be told apart.
function destinationOf(page, entry, start) {
  const click = clickGoes(page, start);
  const enter = enterGoes(page, entry);
  if (click.unknown || enter.unknown) {
    return UNKNOWN;
  }
  if (click.goes !== "" && enter.goes !== "" && click.goes !== enter.goes) {
    return UNKNOWN;
  }
  return click.goes !== "" ? click : enter;
}

// The start of the text inside an element, taken from the text nodes the
// accessibility tree holds below it, in order, up to one character past the cap.
function excerptOf(tree, node) {
  let text = "";
  const pending = [...(node.childIds ?? [])].reverse();
  while (pending.length > 0 && squash(text).length <= SHOWN_CAP) {
    const child = tree.get(pending.pop());
    if (child === undefined) {
      continue;
    }
    if (child.role?.value === "StaticText") {
      text += child.name?.value ?? "";
    } else {
      pending.push(...[...(child.childIds ?? [])].reverse());
    }
  }
  return squash(text);
}

// What the browser has at `point`, in the window: the point is in the window, and
// the browser answers a location in page coordinates, so the page's scroll is added
// for that one question. Nothing at the point is a Failure.
async function hitAt(session, point) {
  const view = (await command(session, "Page.getLayoutMetrics")).cssLayoutViewport;
  if (view === undefined) {
    throw new Failure("failed");
  }
  if (
    point.x < 0 ||
    point.y < 0 ||
    point.x >= view.clientWidth ||
    point.y >= view.clientHeight
  ) {
    throw new Failure("offscreen");
  }
  const hit = await tryRead(session, "DOM.getNodeForLocation", {
    x: point.x + Math.round(view.pageX),
    y: point.y + Math.round(view.pageY),
    includeUserAgentShadowDOM: false,
    ignorePointerEventsNone: false,
  });
  if (hit === null) {
    throw new Failure("offscreen");
  }
  return hit;
}

// The entry of the document `page` for the node `hit` is, if it is the element
// `backend` or inside it, in the page's own frame, and a Failure that says it is
// covered if it is another element, a frame, or a node `page` does not hold. Where
// `page` was read decides what is inside the element: it has to be read after
// `hit`, so that a node the page moved out of the element since is not counted.
function coverOf(page, hit, backend) {
  const start = page.byBackend.get(hit.backendNodeId);
  if (hit.frameId === page.frame) {
    for (let up = start; up; up = parentOf(page, up)) {
      if (up.backend === backend) {
        return start;
      }
    }
  }
  throw new Failure("covered");
}

// Scrolls the element into view, finds the middle of its box, and says whether
// it is the element that a pointer there would reach. The point is in the
// viewport, as an input event is. It answers the point, the rectangle and the node
// the pointer reaches, which is the element or inside it.
async function reach(session, page, entry) {
  await tryRead(session, "DOM.scrollIntoViewIfNeeded", { backendNodeId: entry.backend });
  const box = await tryRead(session, "DOM.getBoxModel", { backendNodeId: entry.backend });
  if (box === null) {
    throw new Failure("hidden");
  }
  const quad = box.model.border;
  const xs = [quad[0], quad[2], quad[4], quad[6]];
  const ys = [quad[1], quad[3], quad[5], quad[7]];
  const point = {
    x: Math.round(xs.reduce((sum, value) => sum + value, 0) / 4),
    y: Math.round(ys.reduce((sum, value) => sum + value, 0) / 4),
  };
  const hit = coverOf(page, await hitAt(session, point), entry.backend);
  return {
    point,
    hit,
    rect: {
      x: Math.round(Math.min(...xs)),
      y: Math.round(Math.min(...ys)),
      width: Math.round(Math.max(...xs) - Math.min(...xs)),
      height: Math.round(Math.max(...ys) - Math.min(...ys)),
    },
  };
}

// The roles of the tree that stand for the page itself. The focus is on the
// page when it is on no element.
const ROOTS = ["RootWebArea", "WebArea"];

// What a resolution holds beside its fields, by resolution: the address the tab
// was at, where its destination was worked out from, and the destination, for
// requireReach.
const kept = new WeakMap();

// The input types a person types text into, as the BraveBot extension takes
// them.
const TEXT_TYPES = ["", "text", "email", "tel", "url", "search"];

// Whether text can be typed into an element: a text area, a text input, or an
// element made editable.
function isEditable(entry) {
  if (entry.tag === "textarea") {
    return true;
  }
  if (entry.tag === "input") {
    return TEXT_TYPES.includes((attribute(entry, "type") ?? "").toLowerCase());
  }
  const editable = attribute(entry, "contenteditable");
  return editable !== null && ["", "true", "plaintext-only"].includes(editable.toLowerCase());
}

// Enables what the reads need and finds the accessibility tree of the tab, which
// has to be at `url`. A page with more nodes than the cap is not looked through.
async function readTree(session, url) {
  await command(session, "Accessibility.enable");
  await command(session, "DOM.enable");
  await command(session, "CSS.enable");
  const { frame } = (await command(session, "Page.getFrameTree")).frameTree;
  if (`${frame.url}${frame.urlFragment ?? ""}` !== url) {
    throw new Failure("left");
  }
  const { nodes } = await command(session, "Accessibility.getFullAXTree");
  if (nodes.length > MAX_NODES) {
    throw new Failure("too_big");
  }
  return { nodes, frame };
}

// The flattened document of the tab, indexed, for the frame `frame`.
async function readDocument(session, frame) {
  const flattened = await command(session, "DOM.getFlattenedDocument", {
    depth: -1,
    pierce: true,
  });
  if (flattened.nodes.length > MAX_NODES) {
    throw new Failure("too_big");
  }
  return { ...indexOf(flattened.nodes), frame: frame.id };
}

// The resolution of `node`, a displayed node of the tree: its refusals, its
// middle and what a pointer there reaches, its description and its fingerprint.
// `hidden` is null, or is how the page names a file input that it hides
// (resolveFileInput), which has no middle to reach and so no point.
// `from` says where the destination starts: at the node the pointer reaches, for an
// action done with the pointer, or at the element, for a key; an action that
// navigates nowhere, such as a key other than Enter, has none.
async function conclude(
  session,
  { page, nodes, node, role, url, from = "pointer", navigates = true, allowFile = false, hidden = null },
) {
  const entry = page.byBackend.get(node.backendDOMNodeId);
  const refusal = refusalOf(entry, node, allowFile);
  if (refusal !== null) {
    throw refusal;
  }
  // A file input the page hides has no middle to reach, and the browser is given
  // the node and not a point, so it is not scrolled to or tested for a cover.
  const { point, hit, rect } =
    hidden === null ? await reach(session, page, entry) : { point: null, hit: entry, rect: null };

  const label = squash(node.name?.value);
  const where = navigates
    ? destinationOf(page, entry, from === "pointer" ? hit : entry)
    : NOWHERE;
  const excerpt = excerptOf(new Map(nodes.map((each) => [each.nodeId, each])), node);
  // The window cuts each to its own limit and says how many characters it left
  // out, which it can say only of a word it was given whole.
  if ([label, excerpt, where.goes].some((word) => word.length > SHOWN_CAP)) {
    throw new Refusal("long");
  }
  const places = [];
  for (let up = parentOf(page, entry); up; up = parentOf(page, up)) {
    places.push(up.backend);
  }
  const resolution = Object.freeze({
    backendNodeId: entry.backend,
    point: Object.freeze(point),
    hidden,
    editable: isEditable(entry),
    fingerprint: JSON.stringify([
      entry.backend,
      entry.tag,
      role,
      label,
      where.goes,
      places,
      where.unknown,
      ...(hidden === null ? [] : [hidden]),
    ]),
    description: Object.freeze({
      tag: entry.tag,
      role,
      name: label,
      excerpt,
      goes: where.goes,
      ...(where.unknown ? { goesUnknown: true } : {}),
      rect: Object.freeze(rect),
    }),
  });
  kept.set(resolution, { url, from, navigates, where });
  return resolution;
}

// The one element of the page in the session's tab that has `role` and the
// accessible name `name`, compared without regard to case and with white space
// collapsed, or a Failure or Refusal that says why there is none. `url` is the
// address the tab was found at; a tab that has moved on from it is a Failure.
//
// It answers {backendNodeId, point, editable, fingerprint, description}. The
// point is the middle of the element in the viewport, in whole pixels. The
// fingerprint is a string that is the same when the same element is resolved
// again, and differs when the node was replaced or moved, or its role, name or
// destination changed. The description is what a window shows: tag, role, name,
// the start of its text, where it goes and its rectangle. The words are whole, up
// to SHOWN_CAP characters, and the window cuts them to its limits; an element with
// a longer one is refused.
export async function resolveElement(session, { url, role, name }) {
  if (!ROLES.includes(role)) {
    throw new Refusal("argument", "role");
  }
  const wanted = squash(name).toLowerCase();
  if (wanted === "") {
    throw new Refusal("argument", "name");
  }
  const { nodes, frame } = await readTree(session, url);
  const named = nodes.filter(
    (node) =>
      AX_ROLES[role].includes(node.role?.value) &&
      squash(node.name?.value).toLowerCase() === wanted,
  );
  if (named.length === 0) {
    throw new Failure("none");
  }
  if (named.length > MAX_CANDIDATES) {
    throw new Failure("several", named.length);
  }

  const page = await readDocument(session, frame);
  const shown = [];
  for (const node of named) {
    if (await isDisplayed(session, page, node)) {
      shown.push(node);
    }
  }
  if (shown.length > 1) {
    throw new Failure("several", shown.length);
  }
  if (shown.length === 0) {
    throw new Failure("hidden", named.length);
  }
  return conclude(session, { page, nodes, node: shown[0], role, url });
}

// The elements a label names, which HTML calls labelable.
const LABELABLE = ["button", "input", "meter", "output", "progress", "select", "textarea"];

const isLabelable = (entry) =>
  LABELABLE.includes(entry.tag) &&
  !(entry.tag === "input" && (attribute(entry, "type") ?? "").toLowerCase() === "hidden");

// The backend node of the document or shadow root an entry is in, which is as far
// as an id and the `for` of a label reach.
function scopeOf(page, entry) {
  let up = parentOf(page, entry);
  while (up && up.type !== DOCUMENT && up.type !== FRAGMENT) {
    up = parentOf(page, up);
  }
  return up?.backend;
}

// Whether an entry is in the top document and not in the document of a frame.
function inTopDocument(page, entry) {
  let up = entry;
  while (up && up.type !== DOCUMENT) {
    up = parentOf(page, up);
  }
  return up !== undefined && up.backend === page.top;
}

// The first element with each id of each document or shadow root, and the labels
// of each file input, in document order: the labels whose `for` names its id, and
// the label that has no `for` and holds it as the first element it can name.
function markupIndex(page) {
  const ids = new Map();
  for (const entry of page.byNode.values()) {
    const id = entry.type === ELEMENT ? attribute(entry, "id") : null;
    if (id !== null && id !== "") {
      const key = `${scopeOf(page, entry)}:${id}`;
      ids.set(key, ids.get(key) ?? entry);
    }
  }
  const labels = new Map();
  const add = (input, label) => labels.set(input, [...(labels.get(input) ?? []), label]);
  const taken = new Set();
  for (const entry of page.byNode.values()) {
    if (entry.type !== ELEMENT) {
      continue;
    }
    if (entry.tag === "label") {
      const target = attribute(entry, "for");
      const named = target === null ? undefined : ids.get(`${scopeOf(page, entry)}:${target}`);
      if (named !== undefined && isFileInput(named)) {
        add(named, entry);
      }
    } else if (isLabelable(entry)) {
      let label = parentOf(page, entry);
      while (label && label.type === ELEMENT && label.tag !== "label") {
        label = parentOf(page, label);
      }
      if (label && label.type === ELEMENT && attribute(label, "for") === null && !taken.has(label)) {
        taken.add(label);
        if (isFileInput(entry)) {
          add(entry, label);
        }
      }
    }
  }
  return { ids, labels };
}

// What the page's markup calls a file input that the browser does not name: the
// elements `aria-labelledby` points to, its `aria-label`, its labels and its
// `title`, in the order the browser takes them, and the first of them that has
// words is the one name, with the source "label". An input none of them names has
// its `name` and its `id`, either of which names it, with the source "identifier".
// The text of another element is the text the tree holds below it, and an element
// that is not in the tree has none. Null where nothing names it.
function markupNames(page, index, textOf, entry) {
  const scope = scopeOf(page, entry);
  const referenced = (attribute(entry, "aria-labelledby") ?? "")
    .split(/\s+/)
    .map((id) => index.ids.get(`${scope}:${id}`))
    .filter((target) => target !== undefined);
  for (const name of [
    squash(referenced.map(textOf).join(" ")),
    squash(attribute(entry, "aria-label")),
    squash((index.labels.get(entry) ?? []).map(textOf).join(" ")),
    squash(attribute(entry, "title")),
  ]) {
    if (name !== "") {
      return { names: [name], source: "label" };
    }
  }
  const identifiers = [attribute(entry, "name"), attribute(entry, "id")]
    .map(squash)
    .filter((name) => name !== "");
  return identifiers.length > 0 ? { names: identifiers, source: "identifier" } : null;
}

// The properties of a node the tree does not hold, for the one that matters to a
// refusal: whether the input is disabled by its own attribute or by a fieldset
// above it.
function markedDisabled(page, entry) {
  for (let up = entry; up && up.type === ELEMENT; up = parentOf(page, up)) {
    if ((up === entry || up.tag === "fieldset") && attribute(up, "disabled") !== null) {
      return [{ name: "disabled", value: { value: true } }];
    }
  }
  return [];
}

// The one file input of the top document of the page in the session's tab that has
// the name `name`, compared without regard to case and with white space collapsed,
// and the role `role` where one is given, or a Failure or Refusal that says why
// there is none. It is resolveElement for the one tool that chooses a file
// (choose_file), and differs in these. The elements looked at are the file inputs of
// the document, found by their tag and their type attribute, since a file input is a
// button in the tree whatever it is called and a call that leaves out the role names
// it by its name alone. `allowFile` is given to conclude, so that the input is not
// refused for being a file chooser, as it is for every other tool, and anything that
// is not one is. And an input the page hides is found: many sites hide the input
// and draw a button or a label that opens it.
//
// An input the tree holds is named as the tree names it. One the tree leaves out
// (display: none, the hidden attribute, visibility: hidden) is named as its own
// markup names it (markupNames), and no script of the page is run. Of the inputs with
// the name, the ones that are displayed (isDisplayed) are counted, and the others are
// counted only where none is displayed, as resolveElement counts only the displayed
// elements. One is the input and two or more are several. Disabled and payment card
// refuse it as they refuse any element. An input the page hides has no middle to
// reach, since the browser is given the node and not a point, so it is not scrolled
// to or tested for a cover, and its resolution says how the page names it: hidden is
// "label" or "identifier", and null for an input that is displayed. The role in the
// resolution is the one asked for, or the tree's own word for the input, or button for
// one the tree leaves out.
export async function resolveFileInput(session, { url, role, name }) {
  if (role !== undefined && !ROLES.includes(role)) {
    throw new Refusal("argument", "role");
  }
  const wanted = squash(name).toLowerCase();
  if (wanted === "") {
    throw new Refusal("argument", "name");
  }
  const { nodes, frame } = await readTree(session, url);
  const page = await readDocument(session, frame);
  const tree = new Map(nodes.map((each) => [each.nodeId, each]));
  const inTree = new Map(
    nodes.filter((each) => each.backendDOMNodeId !== undefined).map((each) => [each.backendDOMNodeId, each]),
  );
  const textOf = (entry) =>
    inTree.has(entry.backend) ? excerptOf(tree, inTree.get(entry.backend)) : "";
  let index = null;
  const named = [];
  for (const entry of page.byBackend.values()) {
    if (!isFileInput(entry)) {
      continue;
    }
    const node = inTree.get(entry.backend);
    const seen = node !== undefined && !node.ignored ? node : undefined;
    const given = seen === undefined ? "" : squash(seen.name?.value);
    let found = given === "" ? null : { names: [given], source: "label" };
    if (found === null && inTopDocument(page, entry)) {
      index ??= markupIndex(page);
      found = markupNames(page, index, textOf, entry);
    }
    const treeRole = seen?.role?.value ?? "button";
    const matched = found?.names.find((each) => each.toLowerCase() === wanted);
    if (matched !== undefined && (role === undefined || AX_ROLES[role].includes(treeRole))) {
      named.push({ entry, node, seen, matched, source: found.source, treeRole });
    }
  }
  if (named.length === 0) {
    throw new Failure("none");
  }
  if (named.length > MAX_CANDIDATES) {
    throw new Failure("several", named.length);
  }
  const shown = [];
  for (const each of named) {
    if (each.node !== undefined && (await isDisplayed(session, page, each.node))) {
      shown.push(each);
    }
  }
  const chosen = shown.length > 0 ? shown : named;
  if (chosen.length > 1) {
    throw new Failure("several", chosen.length);
  }
  const [one] = chosen;
  return conclude(session, {
    page,
    nodes,
    node: {
      ...one.seen,
      backendDOMNodeId: one.entry.backend,
      name: { value: one.matched },
      properties: one.seen?.properties ?? markedDisabled(page, one.entry),
    },
    role: role ?? one.treeRole,
    url,
    allowFile: true,
    hidden: shown.length > 0 ? null : one.source,
  });
}

// The nodes of the tree that have the focus, other than the page itself.
const focusedIn = (nodes) =>
  nodes.filter((node) => property(node, "focused") === true && !ROOTS.includes(node.role?.value));

// The one element of the page in the session's tab that has the focus, with the
// same rules as resolveElement: displayed, not refused, reached by a pointer at
// its middle. Its role is the tree's own word for it. The focus on the page
// itself, on no element, is a Failure, and so is a focus on several. `key` is the
// key to be pressed, if there is one: the element's destination is where Enter
// goes, and any other key goes nowhere.
export async function resolveFocused(session, { url, key }) {
  const { nodes, frame } = await readTree(session, url);
  const focused = focusedIn(nodes);
  if (focused.length === 0) {
    throw new Failure("nofocus");
  }
  if (focused.length > 1) {
    throw new Failure("several", focused.length);
  }
  const page = await readDocument(session, frame);
  if (!(await isDisplayed(session, page, focused[0]))) {
    throw new Failure("hidden", 1);
  }
  const role = String(focused[0].role?.value ?? "");
  return conclude(session, {
    page,
    nodes,
    node: focused[0],
    role,
    url,
    from: "element",
    navigates: key === undefined || key === "Enter",
  });
}

// Fails unless the tab is still at the address `resolution` was made at and a
// pointer at its point would still reach its element, as it did when the
// resolution was made, and lead where it did. Nothing is scrolled, so the point is
// the window's point the input will be sent to. What is inside the element is read
// from the document as it is now, after the browser has said what is at the point,
// so a node the page has moved out of the element since the resolution is not
// counted as inside it. This reads the page and sends nothing.
export async function requireReach(session, resolution) {
  const held = kept.get(resolution);
  if (held === undefined) {
    throw new Failure("failed");
  }
  const { frame } = (await command(session, "Page.getFrameTree")).frameTree;
  if (`${frame.url}${frame.urlFragment ?? ""}` !== held.url) {
    throw new Failure("left");
  }
  const hit = await hitAt(session, resolution.point);
  const page = await readDocument(session, frame);
  const start = coverOf(page, hit, resolution.backendNodeId);
  const entry = page.byBackend.get(resolution.backendNodeId);
  const now = held.navigates
    ? destinationOf(page, entry, held.from === "pointer" ? start : entry)
    : NOWHERE;
  if (now.goes !== held.where.goes || now.unknown !== held.where.unknown) {
    throw new Failure("changed");
  }
}

// Fails unless the tab is still at the address `resolution` was made at and the
// one element that has the focus is the element of the resolution, with the name
// it had. A key and the text of a typing go to whatever has the focus, which a page
// can move at any time, so this is asked just before each of them. This reads the
// page and sends nothing.
export async function requireFocus(session, resolution) {
  const held = kept.get(resolution);
  if (held === undefined) {
    throw new Failure("failed");
  }
  const { frame } = (await command(session, "Page.getFrameTree")).frameTree;
  if (`${frame.url}${frame.urlFragment ?? ""}` !== held.url) {
    throw new Failure("left");
  }
  const { nodes } = await command(session, "Accessibility.getFullAXTree");
  if (nodes.length > MAX_NODES) {
    throw new Failure("too_big");
  }
  const focused = focusedIn(nodes);
  if (
    focused.length !== 1 ||
    focused[0].backendDOMNodeId !== resolution.backendNodeId ||
    squash(focused[0].name?.value) !== resolution.description.name
  ) {
    throw new Failure("changed");
  }
}

// Fails with "changed" unless `again`, a resolution made after the person was
// shown `shown`, is of the same element: the same node in the same place, with
// the same role, name and destination.
export function requireSameElement(shown, again) {
  if (shown.fingerprint !== again.fingerprint) {
    throw new Failure("changed");
  }
}

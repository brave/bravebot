// A small fake page for the tests of the functions the extension injects into
// one: a tree of elements with the parts of the DOM those functions read, a
// document and shadow roots, a hit test, and a log of every event dispatched and
// every call that could run a string as code, read it as a selector or parse it
// as HTML. It is not a browser: it has no layout, no scrolling and no default
// action of an event, and its accessible names are only what the injected
// functions compute from it.
//
//   el(tag, attributes, children, more)   an element; a child may be a string
//   page({ href, body })                  the page, with `install` and `run`
//
// An element the injected function makes with `document.createElement` is an
// ordinary element, and `attachShadow`, `append`, `remove` and `animate` do what
// the function needs of them; the page logs the shadow roots made (`shadows`),
// the animations asked for (`animations`) and the elements removed (`removed`).
//
// `more` sets properties the element reads: `box` ({ left, top, width, height }),
// `style` ({ display, visibility, opacity }), `disabled`, `readOnly`, `hidden`,
// `href`, `form`, `action`, `multiple` and `held` (the value of a field). An
// element without a box is given the next row of a column, 100 by 30, from the
// top of the page, except the html, head and body elements, which have none.
//
// As in a browser, what a node holds is answered by accessors of its prototype
// (the node keeps its state under a symbol), so a property written on the node
// itself stands in front of the accessor. A page's markup does that to a form:
// a control named `action` answers `form.action`. `page().clobber` writes such
// properties on the nodes, so a test can see whether a function reads a node
// through its prototype or through the node. The fake's own code reads the state
// and so is not fooled.

const INLINE = new Set(["a", "span", "b", "i", "em", "strong", "label", "img"]);
const CONTROLS = new Set(["input", "textarea", "select", "button"]);

// Where a node keeps what its accessors answer.
const STATE = Symbol("state");
const state = (node) => node[STATE];

// Makes each of `names` a property of the prototype of `Class` that reads and
// writes the node's state, as the DOM's attributes read and write what the node
// holds.
function declare(Class, names) {
  for (const name of names) {
    if (!Object.hasOwn(Class.prototype, name)) {
      Object.defineProperty(Class.prototype, name, {
        get() {
          return this[STATE][name];
        },
        set(value) {
          this[STATE][name] = value;
        },
        configurable: true,
      });
    }
  }
}

// The nodes under a node that are elements, and the text of a node, read from
// the state.
const kidsOf = (node) => state(node).childNodes.filter((child) => state(child).nodeType === 1);
const textOf = (node) =>
  state(node).nodeType === 3
    ? state(node).data
    : state(node)
        .childNodes.map((child) => textOf(child))
        .join("");
const within = (ancestor, node) => {
  for (let up = node; up; up = state(up).parentNode) {
    if (up === ancestor) {
      return true;
    }
  }
  return false;
};
const rootOf = (node) => {
  let up = node;
  while (state(up).parentNode) {
    up = state(up).parentNode;
  }
  return up;
};

class FakeText {
  constructor(data) {
    this[STATE] = {};
    this.nodeType = 3;
    this.data = data;
    this.parentNode = null;
  }

  get textContent() {
    return textOf(this);
  }
}
declare(FakeText, ["nodeType", "data", "parentNode"]);

// The page that is running an injected function, which every element logs to,
// and the one that was running when it began.
let current = null;
let previous = null;

export class FakeElement {
  constructor(localName, attributes = {}, children = [], more = {}) {
    this[STATE] = {};
    this.nodeType = 1;
    this.localName = localName;
    this.attributes = { ...attributes };
    this.childNodes = children.map((child) =>
      typeof child === "string" ? new FakeText(child) : child,
    );
    for (const child of this.childNodes) {
      child.parentNode = this;
    }
    this.parentNode = null;
    this.style = {
      display: INLINE.has(localName) ? "inline" : "block",
      visibility: "visible",
      opacity: "1",
      ...more.style,
    };
    this.listeners = {};
    const { style, ...rest } = more;
    declare(FakeElement, Object.keys(rest));
    Object.assign(this, rest);
  }

  get parentElement() {
    const up = state(this).parentNode;
    return up && state(up).nodeType === 1 ? up : null;
  }

  get children() {
    return kidsOf(this);
  }

  get textContent() {
    return textOf(this);
  }

  getAttribute(name) {
    const held = state(this).attributes;
    return Object.hasOwn(held, name) ? held[name] : null;
  }

  hasAttribute(name) {
    return Object.hasOwn(state(this).attributes, name);
  }

  attachShadow({ mode }) {
    const root = current.made(new FakeRoot(11, [], current));
    Object.assign(state(root), { mode, host: this, activeElement: null });
    Object.assign(state(this), { shadow: root, shadowRoot: mode === "open" ? root : null });
    current.shadows.push({ host: this, mode });
    return root;
  }

  append(...nodes) {
    for (const node of nodes) {
      state(node).parentNode = this;
      state(this).childNodes.push(node);
    }
  }

  remove() {
    const parent = state(this).parentNode;
    if (parent) {
      const siblings = state(parent).childNodes;
      siblings.splice(siblings.indexOf(this), 1);
      state(this).parentNode = null;
    }
    current.removed.push(this);
  }

  animate(frames, options) {
    current.animations.push({ target: this, frames, options });
  }

  getRootNode() {
    return rootOf(this);
  }

  contains(node) {
    return within(this, node);
  }

  // The controls that belong to a form, in tree order: those whose `form` it is.
  get elements() {
    if (state(this).localName !== "form") {
      return undefined;
    }
    const found = [];
    const visit = (node) => {
      for (const child of kidsOf(node)) {
        if (state(child).form === this) {
          found.push(child);
        }
        visit(child);
      }
    };
    visit(rootOf(this));
    return found;
  }

  get labels() {
    if (!CONTROLS.has(state(this).localName)) {
      return undefined;
    }
    const found = [];
    const own = state(this).attributes;
    const visit = (node) => {
      for (const child of kidsOf(node)) {
        const held = state(child);
        if (
          held.localName === "label" &&
          (within(child, this) ||
            (own.id !== undefined && held.attributes.for === own.id))
        ) {
          found.push(child);
        }
        visit(child);
      }
    };
    visit(rootOf(this));
    return found;
  }

  getBoundingClientRect() {
    const { left, top, width, height } = state(this).box ?? {
      left: 0,
      top: 0,
      width: 0,
      height: 0,
    };
    return { left, top, width, height, right: left + width, bottom: top + height };
  }

  scrollIntoView(options) {
    current.events.push({ type: "scrollIntoView", target: this, options });
  }

  focus() {
    current.active = this;
    current.events.push({ type: "focus", target: this });
  }

  dispatchEvent(event) {
    current.events.push({ type: event.type, target: this, event });
    for (const listener of state(this).listeners[event.type] ?? []) {
      listener(event);
    }
    return true;
  }

  // Everything below is a way to run a string as code, read it as a selector or
  // parse it as HTML. The page log records the string, and nothing is done with it.
  querySelector(selector) {
    current.risky.push({ api: "querySelector", argument: selector });
    return null;
  }

  querySelectorAll(selector) {
    current.risky.push({ api: "querySelectorAll", argument: selector });
    return [];
  }

  matches(selector) {
    current.risky.push({ api: "matches", argument: selector });
    return false;
  }

  closest(selector) {
    current.risky.push({ api: "closest", argument: selector });
    return null;
  }

  insertAdjacentHTML(where, html) {
    current.risky.push({ api: "insertAdjacentHTML", argument: html });
  }

  set innerHTML(html) {
    current.risky.push({ api: "innerHTML", argument: html });
  }

  set outerHTML(html) {
    current.risky.push({ api: "outerHTML", argument: html });
  }
}
declare(FakeElement, [
  "nodeType",
  "localName",
  "attributes",
  "childNodes",
  "parentNode",
  "style",
  "listeners",
  "box",
  "shadow",
  "shadowRoot",
  "hidden",
  "disabled",
  "readOnly",
  "href",
  "form",
  "action",
  "formAction",
  "multiple",
  "scrollHeight",
  "held",
  "picked",
]);

// A text field or a text area: the value is an accessor of the prototype, as in
// a browser, so a function that sets it through the prototype is told apart from
// one that sets it on the element.
export class FakeField extends FakeElement {
  get value() {
    return state(this).held ?? "";
  }

  set value(value) {
    current.events.push({ type: "value", target: this, value });
    state(this).held = value;
  }
}

export class FakeSelect extends FakeElement {
  get options() {
    return kidsOf(this).flatMap((child) =>
      state(child).localName === "optgroup" ? kidsOf(child) : [child],
    );
  }
}

export class FakeOption extends FakeElement {
  get selected() {
    return state(this).picked === true;
  }

  set selected(value) {
    current.events.push({ type: "selected", target: this, value });
    state(this).picked = value;
  }
}

class FakeRoot {
  constructor(nodeType, children, page) {
    this[STATE] = {};
    this.nodeType = nodeType;
    this.page = page;
    this.childNodes = children;
    this.parentNode = null;
    for (const child of children) {
      child.parentNode = this;
    }
  }

  get children() {
    return kidsOf(this);
  }

  append(...nodes) {
    for (const node of nodes) {
      state(node).parentNode = this;
      state(this).childNodes.push(node);
    }
  }

  getElementById(id) {
    current.lookups.push(id);
    const visit = (nodes) => {
      for (const node of nodes) {
        if (state(node).attributes.id === id) {
          return node;
        }
        const found = visit(kidsOf(node));
        if (found) {
          return found;
        }
      }
      return null;
    };
    return visit(kidsOf(this));
  }

  elementFromPoint(x, y) {
    return state(this).page.hit(x, y, this);
  }

  querySelector(selector) {
    current.risky.push({ api: "querySelector", argument: selector });
    return null;
  }

  querySelectorAll(selector) {
    current.risky.push({ api: "querySelectorAll", argument: selector });
    return [];
  }
}
declare(FakeRoot, [
  "nodeType",
  "childNodes",
  "parentNode",
  "page",
  "mode",
  "host",
  "activeElement",
  "documentElement",
  "body",
]);

// The document: what a page answers of itself besides the nodes under it.
class FakeDocument extends FakeRoot {
  get activeElement() {
    return state(this).page.active ?? state(this).body;
  }

  execCommand(name, ui, value) {
    const made = state(this).page;
    made.commands.push({ name, value });
    if (name === "insertText" && made.editing) {
      made.events.push({ type: "insertText", target: made.active, value });
    }
    return made.editing;
  }

  write(text) {
    state(this).page.risky.push({ api: "write", argument: text });
  }

  createElement(name) {
    return state(this).page.made(el(name));
  }
}

// A shadow root holding `children`, in the host the element is given to. A closed
// one is reachable only through `chrome.dom`.
export function shadowed(host, mode, children) {
  const root = new FakeRoot(11, children, null);
  root.mode = mode;
  root.host = host;
  root.activeElement = null;
  host.shadow = root;
  host.shadowRoot = mode === "open" ? root : null;
  return host;
}

export function el(localName, attributes = {}, children = [], more = {}) {
  const Class =
    localName === "input" || localName === "textarea"
      ? FakeField
      : localName === "select"
        ? FakeSelect
        : localName === "option"
          ? FakeOption
          : FakeElement;
  return new Class(localName, attributes, children, more);
}

class FakeEvent {
  constructor(type, init = {}) {
    this.type = type;
    Object.assign(this, init);
  }
}

// What a page's markup puts in front of a property it does not mean to: a node
// that is not a string, a number, a collection or a function. It reads as
// `[object HTMLInputElement]`, as a control named for a property does.
export const HOSTILE = Object.freeze({
  [Symbol.toStringTag]: "HTMLInputElement",
});

// A page at `href` whose body holds `body`. `viewport` is the window's size.
export function page({ href, body, viewport = { width: 1000, height: 800 } }) {
  const html = el("html", {}, [el("head"), el("body", {}, body)]);
  const made = {
    href,
    viewport,
    events: [],
    risky: [],
    lookups: [],
    scrolls: [],
    active: null,
    typed: [],
    commands: [],
    shadows: [],
    animations: [],
    removed: [],
    // What the isolated world holds in its globals between one injection and the next.
    world: {},
    scrollX: 0,
    scrollY: 0,
    editing: true,
  };
  const document = new FakeDocument(9, [html], made);
  document.documentElement = html;
  document.body = kidsOf(html)[1];
  made.document = document;

  // Every element in tree order, shadow trees after the element that hosts them.
  const everything = () => {
    const all = [];
    const visit = (nodes) => {
      for (const node of nodes) {
        all.push(node);
        visit(kidsOf(node));
        if (state(node).shadow) {
          visit(kidsOf(state(node).shadow));
        }
      }
    };
    visit(kidsOf(document));
    return all;
  };
  made.everything = everything;
  let row = 0;
  for (const node of everything()) {
    if (
      state(node).box === undefined &&
      !["html", "head", "body"].includes(state(node).localName)
    ) {
      node.box = { left: 10, top: 10 + 40 * row, width: 100, height: 30 };
      row += 1;
    }
  }
  for (const node of everything()) {
    if (state(node).shadow) {
      state(node).shadow.page = made;
    }
  }

  // The topmost element at a point: the last in tree order whose box holds it
  // and that is displayed, shown as the tree of `root` sees it.
  made.hit = (x, y, root) => {
    let top = null;
    for (const node of everything()) {
      const { box, style } = state(node);
      if (
        box &&
        style.display !== "none" &&
        x >= box.left &&
        x < box.left + box.width &&
        y >= box.top &&
        y < box.top + box.height
      ) {
        top = node;
      }
    }
    while (top && rootOf(top) !== root && state(rootOf(top)).host) {
      top = state(rootOf(top)).host;
    }
    return top;
  };

  // The elements of the tree the injected functions put in, not the page: those
  // directly under the document element that the page did not write.
  made.added = () =>
    kidsOf(html).filter((node) => !["head", "body"].includes(state(node).localName));

  made.element = (id) => {
    const found = everything().find((node) => state(node).attributes.id === id);
    if (!found) {
      throw new Error(`the page has no element with id ${id}`);
    }
    return found;
  };

  // Every node a function can read: the elements, the document and each shadow
  // root.
  const nodes = () => [
    document,
    ...everything().flatMap((node) => (state(node).shadow ? [node, state(node).shadow] : [node])),
  ];
  made.nodes = nodes;

  // Puts `value` in front of the property `name` of `node`, as a browser does
  // for the name of a control of a form.
  const shadow = (node, name, value) =>
    Object.defineProperty(node, name, { value, configurable: true, writable: true });

  // What the markup of a form does: each control under it is the property of the
  // form that has the control's name or id, over the form's own, and likewise
  // for the document and the forms, images and frames that are named.
  made.clobberByName = () => {
    const named = (root) => {
      const found = [];
      const visit = (node) => {
        for (const child of kidsOf(node)) {
          found.push(child);
          visit(child);
        }
      };
      visit(root);
      return found;
    };
    for (const form of everything().filter((node) => state(node).localName === "form")) {
      for (const control of named(form)) {
        for (const name of [state(control).attributes.name, state(control).attributes.id]) {
          if (name) {
            shadow(form, name, control);
          }
        }
      }
    }
    for (const node of everything()) {
      const attributes = state(node).attributes;
      if (["form", "img", "iframe", "embed", "object"].includes(state(node).localName) && attributes.name) {
        shadow(document, attributes.name, node);
      }
    }
  };

  // Every property any node has, in front of the node's own, so that a read of
  // a node by its name finds `HOSTILE`. Only a read through the prototype finds
  // what the node holds. `extra` names properties a function reads that the fake
  // does not define.
  // A node the function makes is clobbered as it is made.
  let everything_ = null;
  const clobberNode = (node, extra) => {
    const names = new Set(extra);
    for (let at = Object.getPrototypeOf(node); at && at !== Object.prototype; at = Object.getPrototypeOf(at)) {
      for (const name of Object.getOwnPropertyNames(at)) {
        names.add(name);
      }
    }
    names.delete("constructor");
    for (const name of names) {
      shadow(node, name, HOSTILE);
    }
    return node;
  };
  made.clobberEverything = (extra = []) => {
    everything_ = extra;
    for (const node of nodes()) {
      clobberNode(node, extra);
    }
  };
  made.made = (node) => (everything_ === null ? node : clobberNode(node, everything_));

  // Runs `run` with the page as the globals an injected function reads, and puts
  // back what was there. `dom: false` is a browser with no `chrome.dom`.
  made.run = (run, { dom = true } = {}) => {
    const names = [
      "document",
      "location",
      "getComputedStyle",
      "innerWidth",
      "innerHeight",
      "scrollX",
      "scrollY",
      "bravebotOutline",
      "scrollTo",
      "scrollBy",
      "chrome",
      "eval",
      "Function",
      "Event",
      "MouseEvent",
      "PointerEvent",
      "KeyboardEvent",
      "InputEvent",
    ];
    const saved = new Map(
      names.map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]),
    );
    const set = (name, value) =>
      Object.defineProperty(globalThis, name, {
        value,
        configurable: true,
        writable: true,
      });
    const forbidden = (api) => (argument) => {
      made.risky.push({ api, argument });
      throw new Error(`${api} was called`);
    };
    previous = current;
    current = made;
    set("document", document);
    set("location", { href: made.href });
    set("getComputedStyle", (node) => ({ ...state(node).style }));
    set("innerWidth", viewport.width);
    set("innerHeight", viewport.height);
    set("scrollX", made.scrollX);
    set("scrollY", made.scrollY);
    if (made.world.outline !== undefined) {
      set("bravebotOutline", made.world.outline);
    }
    set("scrollTo", (options) => made.scrolls.push({ call: "scrollTo", options }));
    set("scrollBy", (options) => made.scrolls.push({ call: "scrollBy", options }));
    set(
      "chrome",
      dom
        ? { dom: { openOrClosedShadowRoot: (node) => state(node).shadow ?? null } }
        : undefined,
    );
    set("eval", forbidden("eval"));
    set("Function", forbidden("Function"));
    for (const name of ["Event", "MouseEvent", "PointerEvent", "KeyboardEvent", "InputEvent"]) {
      set(name, FakeEvent);
    }
    try {
      return run();
    } finally {
      made.world.outline = Object.hasOwn(globalThis, "bravebotOutline")
        ? globalThis.bravebotOutline
        : undefined;
      for (const [name, descriptor] of saved) {
        if (descriptor) {
          Object.defineProperty(globalThis, name, descriptor);
        } else {
          delete globalThis[name];
        }
      }
      current = previous;
    }
  };
  return made;
}

// The attributes a node has, as the page wrote them, whatever the page has put in
// front of its properties.
export const attributesOf = (node) => state(node).attributes;

// The events of one type the page recorded, in order.
export function eventsOf(fake, type) {
  return fake.events.filter((event) => event.type === type);
}

// The kinds of event recorded on `target`, in order, leaving out focus and
// scrolling.
export function kindsOn(fake, target) {
  return fake.events
    .filter(
      (event) =>
        event.target === target &&
        !["focus", "scrollIntoView"].includes(event.type),
    )
    .map((event) => event.type);
}

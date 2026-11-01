// The confirmation window against its service worker, both fake: a browser with
// the windows, ports and runtime the worker uses, and the page confirm.js runs in
// (its markup read from confirm.html, so a missing element fails the test) with
// the events a person causes. The page script is the real one, imported afresh
// for each window the worker opens.
//
//   world(options)          a fake browser and a confirmation over it
//     .confirmation         what createConfirmation made
//     .chrome               the fake `chrome` it was made over
//     .calls                every window call, in order
//     .pages                the pages opened so far; `page` is the last one
//     .open(sender)         opens a page by hand, from a sender, with a window
//   page.element(id)        an element of the page; .shown() is what it holds
//   page.click(id, event)   a click on a control
//   page.scroll(id, top)    the box scrolled to `top`; the options of a page may give
//                           `box`, its scrollHeight, clientHeight and scrollTop
//
// A page and a worker talk over a pair of fake ports that deliver on the next
// microtask and refuse after a disconnect, as real ones do.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createConfirmation, CONFIRMATION_PAGE } from "../confirmation.js";

export const EXTENSION = "fakeextensionid";
export const ADDRESS = `chrome-extension://${EXTENSION}/${CONFIRMATION_PAGE}`;

const markup = readFileSync(new URL("../confirm.html", import.meta.url), "utf8");
const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => id);
const startsHidden = new Set(
  [...markup.matchAll(/<(\w+)[^>]*\bid="([^"]+)"[^>]*\bhidden\b/g)].map(([, , id]) => id),
);
const startsDisabled = new Set(
  [...markup.matchAll(/<button[^>]*\bid="([^"]+)"[^>]*\bdisabled\b/g)].map(([, id]) => id),
);

function listeners() {
  const held = [];
  const addListener = (listener) => held.push(listener);
  return {
    addListener,
    api: { addListener },
    fire: (...values) => held.map((listener) => listener(...values)),
  };
}

// The two ends of a port. A message is a copy, delivered after the sender's
// current work; a disconnect tells the other end.
function ports(name, sender) {
  const make = () => ({
    sent: [],
    disconnected: false,
    onMessage: listeners(),
    onDisconnect: listeners(),
  });
  const page = make();
  const worker = make();
  const wire = (from, to, extra = {}) => {
    from.postMessage = (message) => {
      if (from.disconnected || to.disconnected) {
        throw new Error("Attempting to use a disconnected port object");
      }
      from.sent.push(message);
      const copy = structuredClone(message);
      Promise.resolve().then(() => to.disconnected || to.onMessage.fire(copy));
    };
    from.disconnect = () => {
      if (from.disconnected) {
        return;
      }
      from.disconnected = true;
      to.disconnected = true;
      Promise.resolve().then(() => {
        from.onDisconnect.fire();
        to.onDisconnect.fire();
      });
    };
    Object.assign(from, extra);
  };
  wire(page, worker, { name });
  wire(worker, page, { name, sender });
  return { page, worker };
}

class Element {
  constructor(id) {
    this.id = id;
    this.hidden = startsHidden.has(id);
    this.disabled = startsDisabled.has(id);
    this.className = "";
    this.nodes = [];
    this.handlers = new Map();
    // How a box that scrolls is laid out. A box holds no more than it shows
    // unless a test makes it taller than it is.
    this.scrollTop = 0;
    this.scrollHeight = 100;
    this.clientHeight = 100;
  }

  get textContent() {
    return this.nodes
      .map((node) => (typeof node === "string" ? node : node.textContent))
      .join("");
  }

  set textContent(value) {
    this.nodes = [String(value)];
  }

  replaceChildren(...nodes) {
    this.nodes = nodes;
  }

  addEventListener(type, listener) {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), listener]);
  }

  focus() {
    this.page.focused = this;
  }

  // What the element holds: a string for text, and { mark } for each element of
  // ours, which must be a span marked as such.
  shown() {
    return this.nodes.map((node) => {
      if (typeof node === "string") {
        return node;
      }
      assert.equal(node.localName, "span", "the window made an element that is not a span");
      assert.equal(node.className, "mark");
      assert.deepEqual(node.nodes.length, 1);
      return { mark: node.textContent };
    });
  }
}

class Made extends Element {
  constructor(localName) {
    super(undefined);
    this.localName = localName;
  }
}

// A page of the confirmation window over `ends`, one end of a pair of ports.
async function startPage(
  ends,
  number,
  { focused = true, activation = true, name: manifestName = "BraveBot", box = {} } = {},
) {
  const page = {
    number,
    ports: ends,
    focused: null,
    closed: false,
    elements: new Map(ids.map((id) => [id, new Element(id)])),
    inFront: focused,
    active: activation,
  };
  for (const element of page.elements.values()) {
    element.page = page;
  }
  Object.assign(page.elements.get("typed-box"), box);
  const documentEvents = new Map();
  const windowEvents = new Map();
  const on = (map) => (type, listener) =>
    map.set(type, [...(map.get(type) ?? []), listener]);
  const fire = (map, type, event = {}) =>
    (map.get(type) ?? []).forEach((listener) => listener({ type, ...event }));
  page.fireDocument = (type, event) => fire(documentEvents, type, event);
  page.fireWindow = (type, event) => fire(windowEvents, type, event);
  const document = {
    getElementById(id) {
      assert.ok(page.elements.has(id), `confirm.html has no element with the id ${id}`);
      return page.elements.get(id);
    },
    createElement: (name) => new Made(name),
    title: "",
    addEventListener: on(documentEvents),
    hasFocus: () => page.inFront,
    get visibilityState() {
      return page.visible === false ? "hidden" : "visible";
    },
  };
  page.document = document;
  Object.defineProperty(document, "activeElement", { get: () => page.focused });
  const define = (name, value) =>
    Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
  const globals = {
    document,
    window: {
      addEventListener: on(windowEvents),
      close: () => {
        page.closed = true;
      },
    },
    navigator: {
      userActivation: {
        get isActive() {
          return page.active;
        },
      },
    },
  };
  // The page's globals stay set after it is loaded, for its handlers to use.
  page.use = () => {
    for (const [name, value] of Object.entries(globals)) {
      define(name, value);
    }
  };
  page.use();
  define("chrome", {
    runtime: { connect: () => ends.page, getManifest: () => ({ name: manifestName }) },
  });
  await import(`../confirm.js?window=${number}`);
  delete globalThis.chrome;
  page.element = (id) => page.elements.get(id);
  // The person clicking a control. A click is trusted unless the test says not.
  page.click = async (id, event = {}) => {
    const element = page.elements.get(id);
    page.use();
    for (const listener of element.handlers.get("click") ?? []) {
      listener({ type: "click", isTrusted: true, ...event });
    }
    await flush();
  };
  // The person scrolling the box to `top`.
  page.scroll = async (id, top) => {
    const element = page.elements.get(id);
    element.scrollTop = top;
    page.use();
    for (const listener of element.handlers.get("scroll") ?? []) {
      listener({ type: "scroll" });
    }
    await flush();
  };
  page.key = async (key) => {
    page.use();
    page.fireDocument("keydown", { key, isTrusted: true });
    await flush();
  };
  page.focus = () => {
    page.inFront = true;
    page.fireWindow("focus");
  };
  page.blur = () => {
    page.inFront = false;
    page.fireWindow("blur");
  };
  return page;
}

export const flush = async () => {
  for (let turn = 0; turn < 30; turn += 1) {
    await Promise.resolve();
  }
  await new Promise((resolve) => setImmediate(resolve));
};

let windows = 0;

// A fake browser, and a confirmation over it. `opening` is "page" to open the
// page of each window the worker creates, "none" to leave that to the test, and
// "never" for a browser that does not say whether the window opened until the test
// calls `fake.openLater()`. `closing` is "never" for a browser that does not answer
// `windows.remove`. `failing` is an error `windows.create` throws. `confirmationPage` is the page
// the confirmation is made to show, and `page` is the options of the pages that
// open: `name` is the manifest name the page reads.
export function world({
  wait,
  opening = "page",
  closing = "closes",
  failing,
  confirmationPage,
  page: pageOptions,
} = {}) {
  const calls = [];
  const removed = listeners();
  const connect = listeners();
  const open = new Map();
  const pages = [];
  let nextId = 40;
  const contexts = [];
  const chrome = {
    runtime: {
      id: EXTENSION,
      getURL: (path) => `chrome-extension://${EXTENSION}/${path}`,
      onConnect: connect.api,
      async getContexts(filter) {
        calls.push(["runtime.getContexts", filter]);
        return contexts;
      },
    },
    windows: {
      onRemoved: removed.api,
      async create(options) {
        calls.push(["windows.create", options]);
        if (failing) {
          throw failing;
        }
        const made = { id: (nextId += 1), tabs: [{ id: nextId * 10 }] };
        open.set(made.id, made);
        fake.created.push(made);
        if (opening === "never") {
          return new Promise((resolve) => {
            fake.openLater = () => resolve(made);
          });
        }
        if (opening === "page") {
          Promise.resolve().then(() => fake.open({ window: made }));
        }
        return made;
      },
      async remove(id) {
        calls.push(["windows.remove", id]);
        if (closing === "never") {
          return new Promise(() => {});
        }
        if (!open.has(id)) {
          throw new Error(`No window with id: ${id}.`);
        }
        fake.close(id);
      },
    },
  };
  const confirmation = createConfirmation(chrome, {
    ...(wait === undefined ? {} : { wait }),
    ...(confirmationPage === undefined ? {} : { page: confirmationPage }),
  });
  confirmation.listen();
  const fake = {
    chrome,
    calls,
    confirmation,
    pages,
    created: [],
    contexts,
    get page() {
      return pages.at(-1);
    },
    windowsOpen: () => [...open.keys()],
    // Opens the page of a window, from the sender a real one has unless the test
    // gives another, and hands its port to the worker.
    // `early` is what the page sends the moment it connects, before the worker
    // has finished deciding whether it is the window.
    async open({ window: made, sender, name = "bravebot-confirmation", early = [], ...rest } = {}) {
      const pageAddress = `chrome-extension://${EXTENSION}/${confirmationPage ?? CONFIRMATION_PAGE}`;
      const ends = ports(name, {
        id: EXTENSION,
        url: pageAddress,
        frameId: 0,
        tab: { id: made?.tabs[0].id ?? 1, windowId: made?.id ?? 1 },
        ...sender,
      });
      const page = await startPage(ends, (windows += 1), { ...pageOptions, ...rest });
      page.opened = made;
      pages.push(page);
      connect.fire(ends.worker);
      for (const message of early) {
        ends.page.postMessage(message);
      }
      await flush();
      return page;
    },
    // The window is gone: the browser tells the worker and ends the ports.
    close(id) {
      open.delete(id);
      for (const page of pages) {
        if (page.opened?.id === id) {
          page.ports.page.disconnect();
        }
      }
      removed.fire(id);
    },
  };
  return fake;
}

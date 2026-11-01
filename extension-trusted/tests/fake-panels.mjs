// The two windows of the extension against the service worker that opens them,
// all fake: a browser with the windows, ports and runtime the worker uses, and
// the pages confirm.js and dialog.js run in (their markup read from confirm.html
// and dialog.html, so a missing element fails the test) with the events a person
// causes. The page scripts are the real ones, imported afresh for each window.
// fake-window.mjs does the same for the confirmation window over the shared
// confirmation alone.
//
//   browser(options)        a fake `chrome` for createWindows
//     .chrome  .calls  .pages  .created  .contexts
//     .open({window, sender, early})   opens the page of a window by hand
//     .close(id)                       the window is gone
//   page.element(id)        an element; .shown() is what it holds
//   page.click(id, event)   a click on a control
//   page.key(key)           a key
//   page.type(id, text)     the person typing in a field

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

export const EXTENSION = "fakesecondextension";
export const addressOf = (page) => `chrome-extension://${EXTENSION}/${page}`;

const markupOf = (file) => readFileSync(new URL(`../${file}`, import.meta.url), "utf8");

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
  constructor(id, markup) {
    this.id = id;
    this.hidden = markup.hidden.has(id);
    this.disabled = markup.disabled.has(id);
    this.className = "";
    this.value = "";
    this.nodes = [];
    this.handlers = new Map();
    // A box holds no more than it shows (fake-window.mjs lays one out taller).
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
    super(undefined, { hidden: new Set(), disabled: new Set() });
    this.localName = localName;
  }
}

export const flush = async () => {
  for (let turn = 0; turn < 30; turn += 1) {
    await Promise.resolve();
  }
  await new Promise((resolve) => setImmediate(resolve));
};

let windows = 0;

// A page of a window over `ends`, one end of a pair of ports. `file` is the
// page, such as "dialog.html", and its script is the file of the same name.
async function startPage(
  file,
  ends,
  { focused = true, activation = true, name: manifestName = "BraveBot Trusted Input" } = {},
) {
  const text = markupOf(file);
  const markup = {
    ids: [...text.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => id),
    hidden: new Set(
      [...text.matchAll(/<(\w+)[^>]*\bid="([^"]+)"[^>]*\bhidden\b/g)].map(([, , id]) => id),
    ),
    disabled: new Set(
      [...text.matchAll(/<button[^>]*\bid="([^"]+)"[^>]*\bdisabled\b/g)].map(([, id]) => id),
    ),
  };
  windows += 1;
  const page = {
    file,
    ports: ends,
    focused: null,
    closed: false,
    elements: new Map(markup.ids.map((id) => [id, new Element(id, markup)])),
    inFront: focused,
    active: activation,
  };
  for (const element of page.elements.values()) {
    element.page = page;
  }
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
      assert.ok(page.elements.has(id), `${file} has no element with the id ${id}`);
      return page.elements.get(id);
    },
    createElement: (made) => new Made(made),
    title: "",
    addEventListener: on(documentEvents),
    hasFocus: () => page.inFront,
    get visibilityState() {
      return page.visible === false ? "hidden" : "visible";
    },
  };
  page.document = document;
  Object.defineProperty(document, "activeElement", { get: () => page.focused });
  const define = (key, value) =>
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
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
    for (const [key, value] of Object.entries(globals)) {
      define(key, value);
    }
  };
  page.use();
  define("chrome", {
    runtime: { connect: () => ends.page, getManifest: () => ({ name: manifestName }) },
  });
  await import(`../${file.replace(".html", ".js")}?window=${windows}`);
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
  page.key = async (key) => {
    page.use();
    page.fireDocument("keydown", { key, isTrusted: true });
    await flush();
  };
  page.type = (id, value) => {
    page.elements.get(id).value = value;
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

// A fake `chrome` for createWindows. `opening` is true to open the page of each
// window the worker creates, false to leave that to the test. `failing` is an
// error `windows.create` throws. `page` is the options of the pages that open.
export function browser({ opening = true, failing, page: pageOptions } = {}) {
  const calls = [];
  const removed = listeners();
  const connect = listeners();
  const open = new Map();
  const pages = [];
  const pageOfWindow = new Map();
  const contexts = [];
  let nextId = 40;
  const chrome = {
    runtime: {
      id: EXTENSION,
      getURL: (path) => addressOf(path),
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
        pageOfWindow.set(made.id, options.url.split("/").pop());
        fake.created.push(made);
        if (opening) {
          Promise.resolve().then(() => fake.open({ window: made }));
        }
        return made;
      },
      async remove(id) {
        calls.push(["windows.remove", id]);
        if (!open.has(id)) {
          throw new Error(`No window with id: ${id}.`);
        }
        fake.close(id);
      },
    },
  };
  const fake = {
    chrome,
    calls,
    pages,
    created: [],
    contexts,
    get page() {
      return pages.at(-1);
    },
    windowsOpen: () => [...open.keys()],
    // Opens the page of a window, from the sender a real one has unless the test
    // gives another, and hands its port to the worker. `early` is what the page
    // sends the moment it connects, before the worker has decided whether it is
    // the window; the worker receives it after the connection, as a real one
    // does. `file` is the page to open when the window is not one the worker
    // created.
    async open({ window: made, file, sender, name = "bravebot-confirmation", early = [], ...rest } = {}) {
      const target = file ?? pageOfWindow.get(made?.id) ?? "confirm.html";
      const ends = ports(name, {
        id: EXTENSION,
        url: addressOf(target),
        frameId: 0,
        tab: { id: made?.tabs[0].id ?? 1, windowId: made?.id ?? 1 },
        ...sender,
      });
      const page = await startPage(target, ends, { ...pageOptions, ...rest });
      page.opened = made;
      pages.push(page);
      for (const message of early) {
        ends.page.postMessage(message);
      }
      connect.fire(ends.worker);
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

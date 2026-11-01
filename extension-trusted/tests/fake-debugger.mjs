// A fake browser for the extension: the debugger API with a log of every call,
// and the runtime event the worker listens on. The debugger refuses as the real
// one does: a command on a tab that is not attached, a second attach.

// A fake `chrome.debugger`. `targets` is what getTargets answers, `answers`
// gives a method's answer from its parameters (a promise that has not settled
// holds the command, as an open dialog holds an input), and `failOn` makes a
// call throw: `attach` and `detach` each take an Error, and `sendCommand` takes a
// function from the method to an Error or nothing. A detach that fails has
// detached all the same, as one does when the tab has gone. `emit(method,
// params, tabId)` sends an event to the listeners, as the browser does.
export function fakeDebugger({ targets = [], answers = {}, failOn = {} } = {}) {
  const calls = [];
  const attached = new Set();
  const watchers = new Set();
  const api = {
    async getTargets() {
      calls.push({ call: "getTargets" });
      return structuredClone(targets);
    },
    async attach({ tabId }, version) {
      calls.push({ call: "attach", tabId, version });
      if (failOn.attach) {
        throw failOn.attach;
      }
      if (attached.has(tabId)) {
        throw new Error(
          `Another debugger is already attached to the tab with id: ${tabId}.`,
        );
      }
      attached.add(tabId);
    },
    async detach({ tabId }) {
      calls.push({ call: "detach", tabId });
      const was = attached.delete(tabId);
      if (failOn.detach) {
        throw failOn.detach;
      }
      if (!was) {
        throw new Error(
          `Debugger is not attached to the tab with id: ${tabId}.`,
        );
      }
    },
    async sendCommand({ tabId }, method, params) {
      calls.push({ call: "sendCommand", tabId, method, params });
      if (!attached.has(tabId)) {
        throw new Error(
          `Debugger is not attached to the tab with id: ${tabId}.`,
        );
      }
      const failure = failOn.sendCommand?.(method);
      if (failure) {
        throw failure;
      }
      return answers[method]?.(params) ?? {};
    },
    onEvent: {
      addListener: (listener) => watchers.add(listener),
      removeListener: (listener) => watchers.delete(listener),
    },
  };
  const emit = (method, params, tabId) => {
    for (const watcher of [...watchers]) {
      watcher({ tabId }, method, params);
    }
  };
  return { chrome: { debugger: api }, calls, attached, watchers, emit };
}

// The calls of one kind, in order.
export function callsOf(fake, call) {
  return fake.calls.filter((entry) => entry.call === call);
}

// A page target of the debugger, as getTargets lists it.
export function pageAt(url, tabId, title = "A page") {
  return { type: "page", id: `page-${tabId}`, tabId, url, title, attached: false };
}

// A fake browser for the service worker: `chrome.runtime` and `chrome.windows`
// hold the events and calls the worker is meant to use, so any other listener
// it registers throws. dispatch() sends a message to the external-message
// listener the way the browser does and says whether any reply came back. A
// listener has to return true to reply later. The connections the windows
// listen for, the removals of windows and the calls to the windows API are
// recorded.
export function fakeBrowser(options) {
  const fake = fakeDebugger(options);
  const listeners = [];
  const connections = [];
  const removals = [];
  const windowCalls = [];
  const address = (path) => `chrome-extension://fakesecondextension/${path}`;
  fake.listeners = listeners;
  fake.connections = connections;
  fake.removals = removals;
  fake.windowCalls = windowCalls;
  fake.contexts = [];
  fake.chrome.runtime = {
    id: "fakesecondextension",
    getURL: address,
    async getContexts(filter) {
      windowCalls.push(["getContexts", filter]);
      return fake.contexts;
    },
    onMessageExternal: {
      addListener(listener) {
        listeners.push(listener);
      },
    },
    onConnect: {
      addListener(listener) {
        connections.push(listener);
      },
    },
  };
  fake.chrome.windows = {
    onRemoved: {
      addListener(listener) {
        removals.push(listener);
      },
    },
    async create(created) {
      windowCalls.push(["create", created]);
      return { id: 9, tabs: [{ id: 90 }] };
    },
    async remove(id) {
      windowCalls.push(["remove", id]);
    },
  };
  fake.dispatch = (message, sender) =>
    new Promise((resolve) => {
      let replied = false;
      const sendResponse = (response) => {
        replied = true;
        resolve({ replied: true, response });
      };
      const keepOpen = listeners[0](message, sender, sendResponse);
      if (keepOpen !== true && !replied) {
        resolve({ replied: false });
      }
    });
  return fake;
}

// The answers of a page that was recorded in a real browser (recorded/world.json),
// for fakeDebugger({ answers }). The reads that resolve.js makes are answered from
// the record: the trees as the browser gave them, and for each element its box,
// its style and what is at the middle of it. The page is scrolled as the browser
// scrolls it for scrollIntoViewIfNeeded: vertically, to the middle of the
// window when the element is not wholly in it, and no further than the end of
// the page. A box is in the window's coordinates, and what is at a location is
// asked for in the page's, as the browser asks. `world` is read at each call,
// so a test can change it between two resolutions: replace a node, move it,
// change an address.
export function recordedPage(world) {
  const scroll = { x: 0, y: 0 };
  const view = () => world.metrics.cssLayoutViewport;
  const border = (backendNodeId) => {
    const box = world.boxes[backendNodeId];
    if (!box) {
      throw new Error("Could not compute box model.");
    }
    return box.border;
  };
  const answers = {
    "Accessibility.enable": () => ({}),
    "DOM.enable": () => ({}),
    "CSS.enable": () => ({}),
    "Page.getFrameTree": () => structuredClone(world.frameTree),
    "Accessibility.getFullAXTree": () => ({ nodes: structuredClone(world.ax) }),
    "DOM.getFlattenedDocument": () => ({ nodes: structuredClone(world.flat) }),
    "CSS.getComputedStyleForNode": ({ nodeId }) => {
      const style = world.styles[nodeId];
      if (!style) {
        throw new Error("Node is not an element");
      }
      return {
        computedStyle: Object.entries(style).map(([name, value]) => ({ name, value })),
      };
    },
    "DOM.getBoxModel": ({ backendNodeId }) => {
      const box = world.boxes[backendNodeId];
      border(backendNodeId);
      return {
        model: {
          border: box.border.map((value, at) => (at % 2 === 0 ? value - scroll.x : value - scroll.y)),
          width: box.width,
          height: box.height,
        },
      };
    },
    "Page.getLayoutMetrics": () => ({
      cssLayoutViewport: { ...view(), pageX: scroll.x, pageY: scroll.y },
      cssContentSize: world.metrics.cssContentSize,
    }),
    "DOM.scrollIntoViewIfNeeded": ({ backendNodeId }) => {
      const quad = border(backendNodeId);
      const top = Math.min(quad[1], quad[3], quad[5], quad[7]);
      const bottom = Math.max(quad[1], quad[3], quad[5], quad[7]);
      const { clientHeight } = view();
      if (top >= scroll.y && bottom <= scroll.y + clientHeight) {
        return {};
      }
      const end = Math.max(0, world.metrics.cssContentSize.height - clientHeight);
      scroll.y = Math.min(end, Math.max(0, Math.round((top + bottom) / 2 - clientHeight / 2)));
      return {};
    },
    "DOM.getNodeForLocation": ({ x, y }) => {
      const { clientWidth, clientHeight } = view();
      const hit = world.hits[`${x},${y}`];
      if (!hit || x < scroll.x || y < scroll.y || x >= scroll.x + clientWidth || y >= scroll.y + clientHeight) {
        throw new Error("No node found at given location");
      }
      return { ...hit };
    },
  };
  return { answers, scroll };
}

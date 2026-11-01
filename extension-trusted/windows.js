// The service worker's side of the two windows the person answers in: the
// window that asks whether to act on an element (confirm.html) and the panel
// that asks how to answer a dialog the page opened (dialog.html). Both are the
// shared confirmation (confirmation.js) made for a page of their own.
//
// The browser tells a worker of every port an extension page opens, and the
// shared confirmation sends away any port it does not own. So the one listener
// here hands each port to the confirmation made for the page it came from, and
// to none when there is none; a confirmation never sees the port of the other
// window. A port from a page with no question waiting is sent away, which closes
// the page.
//
// The shared confirmation throws errors with sentences of its own. They are
// turned into the codes of sentences.js here, so that a reply is made from a
// code and never from a message.

import { CONFIRMATION_PORT, CONFIRMATION_WAIT_MS } from "./approval.js";
import { createConfirmation } from "./confirmation.js";
import { Refusal as NotAllowed, ToolError } from "./errors.js";
import { Failure, Refusal } from "./sentences.js";

// The pages of the two windows.
export const ACTION_PAGE = "confirm.html";
export const DIALOG_PAGE = "dialog.html";

// The code of this extension's table for each way the shared confirmation says
// an action was not allowed, by the start of its sentence.
const REASONS = [
  [/^you denied the action in the browser/, "denied"],
  [/^the confirmation window was closed/, "closed"],
  [/^no answer was given in the browser within /, "timed_out"],
  [/^another action is waiting for your answer/, "waiting"],
];

// The error of this extension's table for what the shared confirmation threw.
// A refusal whose sentence is not one of REASONS, and anything that is not one
// of its two errors, is the generic failure.
export function reasonOf(error) {
  if (error instanceof NotAllowed) {
    const found = REASONS.find(([pattern]) => pattern.test(error.message));
    return found === undefined ? new Failure("failed") : new Refusal(found[1]);
  }
  if (error instanceof ToolError) {
    return new Failure("no_window");
  }
  return error;
}

// The page of a port, without its query or fragment.
const pageOf = (port) => String(port.sender?.url).split(/[?#]/)[0];

// A port closed before the window that opened it was asked for anything.
function sendAway(port) {
  if (port.name !== CONFIRMATION_PORT) {
    return;
  }
  try {
    port.postMessage({ type: "ended" });
    port.disconnect();
  } catch {
    // The port closed first.
  }
}

// `port` as the confirmation sees it, with the answer the dialog panel sends
// handed to `record` once the confirmation has taken the port for its window.
// The confirmation sends `show` to that port and to no other.
function watched(port, record) {
  let admitted = false;
  return {
    name: port.name,
    sender: port.sender,
    postMessage(message) {
      if (message?.type === "show") {
        admitted = true;
      }
      return port.postMessage(message);
    },
    disconnect: () => port.disconnect(),
    onMessage: {
      addListener: (listener) =>
        port.onMessage.addListener((message) => {
          if (admitted && message?.type === "answer") {
            record(message);
          }
          return listener(message);
        }),
    },
    onDisconnect: { addListener: (listener) => port.onDisconnect.addListener(listener) },
  };
}

// The windows for the browser `chrome`.
//
//   ask(resolution)             asks about an action, with the time of
//                               CONFIRMATION_WAIT_MS; answers with a token or
//                               throws an error of sentences.js
//   askDialog(dialog, where)    asks how to answer `dialog`, a dialog the page
//                               opened (cdp.js dialogOf), in the time that is
//                               left of the call
//   listen()                    registers the listeners, as the worker starts
//   closeLeftovers()            closes windows an earlier worker left open
export function createWindows(chrome) {
  const connections = new Map();
  const removals = new Set();
  // When the person was first asked in a call, by the resolution asked about.
  const asked = new WeakMap();

  // The browser as the confirmation of `page` sees it. `record` is given the
  // answer of the window, for a panel that takes more from the person than
  // allow or deny. `dispose` takes the listeners away, for a confirmation made
  // for one question.
  function viewOf(page, record) {
    const address = chrome.runtime.getURL(page);
    const removed = [];
    let entry = null;
    const view = {
      runtime: {
        id: chrome.runtime.id,
        getURL: (path) => chrome.runtime.getURL(path),
        getContexts: (filter) => chrome.runtime.getContexts(filter),
        onConnect: {
          addListener: (listener) => {
            entry = (port) => listener(record ? watched(port, record) : port);
            connections.set(address, entry);
          },
        },
      },
      windows: {
        create: (options) => chrome.windows.create(options),
        remove: (id) => chrome.windows.remove(id),
        onRemoved: {
          addListener: (listener) => {
            removals.add(listener);
            removed.push(listener);
          },
        },
      },
    };
    const dispose = () => {
      if (connections.get(address) === entry) {
        connections.delete(address);
      }
      removed.forEach((listener) => removals.delete(listener));
    };
    return { view, dispose };
  }

  const action = createConfirmation(viewOf(ACTION_PAGE).view, { page: ACTION_PAGE });
  const leftovers = createConfirmation(viewOf(DIALOG_PAGE).view, { page: DIALOG_PAGE });

  async function ask(resolution) {
    if (typeof resolution === "object" && resolution !== null) {
      asked.set(resolution, Date.now());
    }
    try {
      return await action.ask(resolution);
    } catch (error) {
      throw reasonOf(error);
    }
  }

  // `where` is the origin and the address of the page, and `first` is the
  // resolution of the action whose window was asked first in this call. The
  // person has until CONFIRMATION_WAIT_MS after that window, so the panel counts
  // down to the same time. A dialog the person does not answer stays open and
  // the call fails with dialog_open, whatever the reason: no answer, a closed
  // panel, a panel that could not be opened, or no time left.
  async function askDialog(dialog, { origin, url, first }) {
    const wait = (asked.get(first) ?? 0) + CONFIRMATION_WAIT_MS - Date.now();
    if (!(wait > 0)) {
      throw new Failure("dialog_open");
    }
    const description = Object.freeze({
      type: dialog.type,
      message: dialog.message,
      defaultPrompt: dialog.defaultPrompt,
      origin,
      url,
    });
    const resolution = Object.freeze({ dialog, description });
    let choice = null;
    const record = (message) => {
      choice ??= {
        accept: message.accept === true,
        text: typeof message.text === "string" ? message.text : "",
      };
    };
    const { view, dispose } = viewOf(DIALOG_PAGE, record);
    const panel = createConfirmation(view, { wait, page: DIALOG_PAGE });
    panel.listen();
    try {
      const token = await panel.ask(resolution);
      if (choice === null) {
        throw new Failure("dialog_open");
      }
      return Object.freeze({ resolution, token, ...choice });
    } catch {
      throw new Failure("dialog_open");
    } finally {
      dispose();
    }
  }

  function listen() {
    chrome.runtime.onConnect.addListener((port) => (connections.get(pageOf(port)) ?? sendAway)(port));
    chrome.windows.onRemoved.addListener((id) => {
      for (const listener of [...removals]) {
        listener(id);
      }
    });
    action.listen();
  }

  async function closeLeftovers() {
    await Promise.all([action.closeLeftovers(), leftovers.closeLeftovers()]);
  }

  return { ask, askDialog, listen, closeLeftovers };
}

// The panel that asks about a dialog, for the tools. background.js sets it as
// the worker starts, as it sets the confirmation of approval.js, and until then
// nobody answers a dialog.
const nobody = async () => {
  throw new Failure("dialog_open");
};
let askPanel = nobody;

export function setDialogPanel(ask = nobody) {
  askPanel = ask;
}

export function askDialog(dialog, where) {
  return askPanel(dialog, where);
}

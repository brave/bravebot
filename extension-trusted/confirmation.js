// The service worker's side of the confirmation window: opens it for one
// resolved element, waits for the person's answer, and gives the tool that acts
// a token for that resolution only when the person allowed it.
//
// The only way to an allow is a message over a port from the window this opens,
// received while it waits. Nothing a web page can do reaches it: the window is
// an extension page that is not web accessible, the port is accepted from that
// page in that window alone, and the page the action is for is not consulted.
// Everything else ends in a denial that names its reason: the person denies,
// closes the window or does not answer within CONFIRMATION_WAIT_MS, the window
// cannot be opened, or this worker stops, in which case the call is lost with the
// worker's connection to the host and the window, finding the port closed, ends
// itself.
//
// A browser that does not say whether it opened the window, or does not answer the
// closing of it, does not hold the call: the time limit ends the question, and the
// closing is waited for at most CLOSE_WAIT_MS. A window that opens after the
// question has ended finds none waiting, and ends itself.
//
// While the person decides, the worker is kept running by the open native
// messaging port, which the browser counts as activity, and by the window's
// messages every HEARTBEAT_MS, each of which is an event the worker receives.

import {
  CONFIRMATION_PORT,
  CONFIRMATION_WAIT_MS,
  approve,
} from "./approval.js";
import { Refusal, ToolError } from "./errors.js";

// The page of the window unless the extension names another, and its size.
export const CONFIRMATION_PAGE = "confirm.html";
const WINDOW_SIZE = Object.freeze({ width: 600, height: 640 });

// How long ask() waits, once the question has ended, for the browser to close the
// window, in milliseconds. A browser that does not answer does not hold the call.
// It is added to the person's 60 seconds in the bounds of a call that the wait of
// the BraveBot extension for the second extension holds (cdp.js CALL_WAIT_MS).
export const CLOSE_WAIT_MS = 1_000;

// Why an action was not allowed. Each is a sentence of ours, with nothing of the
// page in it.
const NOT_ALLOWED = Object.freeze({
  denied: "you denied the action in the browser, so nothing was done",
  closed: "the confirmation window was closed, so nothing was done",
  timeout: `no answer was given in the browser within ${CONFIRMATION_WAIT_MS / 1000} seconds, so nothing was done`,
});

// A confirmation for the browser `chrome`. `wait` is how long the person has, and
// `page` is the extension page the window shows.
//
//   ask(resolution)   opens the window, waits, and answers with approve(resolution)
//                     when the person allows, or throws a Refusal saying why not
//   listen()          registers the listeners the window needs, as the worker starts
//   closeLeftovers()  closes windows of this page a worker that stopped left open
export function createConfirmation(
  chrome,
  { wait = CONFIRMATION_WAIT_MS, page = CONFIRMATION_PAGE } = {},
) {
  const address = chrome.runtime.getURL(page);
  // The window being waited on, or null. Only one is open at a time.
  let pending = null;

  const settle = (state, outcome) => state.settle(outcome);

  async function ask(resolution) {
    if (pending !== null) {
      throw new Refusal(
        "another action is waiting for your answer in the browser, so nothing was done",
      );
    }
    const state = {
      resolution,
      port: null,
      opened: null,
      window: null,
      settle: null,
      deadline: Date.now() + wait,
    };
    pending = state;
    const outcome = new Promise((resolve) => {
      state.settle = resolve;
    });
    const timer = setTimeout(settle, wait, state, "timeout");
    try {
      state.opened = chrome.windows.create({
        url: address,
        type: "popup",
        focused: true,
        ...WINDOW_SIZE,
      });
      state.opened.then(
        (opened) => {
          state.window = opened;
        },
        (error) => settle(state, error),
      );
      const ended = await outcome;
      if (ended === "allowed") {
        return approve(resolution);
      }
      if (ended instanceof Error) {
        throw new ToolError(
          `the confirmation window could not be opened: ${ended.message}`,
        );
      }
      throw new Refusal(NOT_ALLOWED[ended]);
    } finally {
      clearTimeout(timer);
      pending = null;
      await closeWindow(state);
    }
  }

  // Closes the window of `state`, if it has opened, and waits no longer than
  // CLOSE_WAIT_MS for the browser to do it. A window that has not opened is not
  // waited for: if the browser opens it later, it finds no question waiting and
  // ends itself. A window already gone is not an error.
  async function closeWindow(state) {
    if (state.window === null) {
      return;
    }
    let timer;
    try {
      await Promise.race([
        chrome.windows.remove(state.window.id),
        new Promise((resolve) => {
          timer = setTimeout(resolve, CLOSE_WAIT_MS);
        }),
      ]);
    } catch {
      // Already closed.
    } finally {
      clearTimeout(timer);
    }
  }

  // Whether `port` comes from the window opened for `state`: from this
  // extension, from the page of the window, in its top frame, and from the window
  // that was opened.
  async function isTheWindow(port, state) {
    const sender = port.sender;
    let opened;
    try {
      opened = await state.opened;
    } catch {
      return false;
    }
    return (
      sender?.id === chrome.runtime.id &&
      typeof sender.url === "string" &&
      sender.url.split(/[?#]/)[0] === address &&
      (sender.frameId ?? 0) === 0 &&
      sender.tab?.windowId === opened?.id
    );
  }

  // A port opened to this worker. It is the window's if it passes isTheWindow
  // while a question is waiting and no other port has been taken for it. What it
  // sends is handled in order after that check, and a port that fails it is sent
  // "ended", which closes the window it came from.
  function connected(port) {
    if (port.name !== CONFIRMATION_PORT) {
      return;
    }
    const state = pending;
    const admitted =
      state !== null && state.port === null
        ? isTheWindow(port, state).then((yes) => {
            if (yes && pending === state && state.port === null) {
              state.port = port;
              return true;
            }
            return false;
          })
        : Promise.resolve(false);
    admitted.then((yes) => {
      try {
        if (yes) {
          port.postMessage({
            type: "show",
            description: state.resolution.description,
            deadline: state.deadline,
          });
        } else {
          port.postMessage({ type: "ended" });
          port.disconnect();
        }
      } catch {
        // The port closed while it was checked.
      }
    });
    port.onMessage.addListener(async (message) => {
      if ((await admitted) && pending === state && message?.type === "answer") {
        settle(state, message.allow === true ? "allowed" : "denied");
      }
    });
    port.onDisconnect.addListener(async () => {
      if ((await admitted) && pending === state) {
        settle(state, "closed");
      }
    });
  }

  function removed(windowId) {
    const state = pending;
    if (state === null) {
      return;
    }
    state.opened
      ?.then((opened) => {
        if (opened.id === windowId) {
          settle(state, "closed");
        }
      })
      .catch(() => {});
  }

  function listen() {
    chrome.runtime.onConnect.addListener(connected);
    chrome.windows.onRemoved.addListener(removed);
  }

  async function closeLeftovers() {
    try {
      const contexts = await chrome.runtime.getContexts({ contextTypes: ["TAB"] });
      for (const context of contexts) {
        if (context.documentUrl?.split(/[?#]/)[0] === address && context.windowId >= 0) {
          await chrome.windows.remove(context.windowId).catch(() => {});
        }
      }
    } catch {
      // A browser without getContexts has no way to find them; the window ends itself.
    }
  }

  return { ask, listen, closeLeftovers };
}

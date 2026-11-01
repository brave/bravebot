// The browser's debugger, used in one place. This is the only file of the
// extension that names it, and command() is the only function that sends the
// browser a method. A method has to be on the list below, and one that sends
// input is sent only with a token that the approval given to withSession
// accepts. A session lasts for one call: it is attached when the call starts and
// detached when it ends, however it ends.
//
// The debugger reads every tab's address and title as part of getTargets. Only
// a count and the number of the one tab at the address asked for leave
// tabAt(), and nothing here logs, stores or returns the rest.

import { spend } from "./approval.js";
import { problemWithFile } from "./files.js";
import { Failure, Refusal } from "./sentences.js";
import { TYPED_TEXT_LIMIT } from "./words.js";

// The version of the debugging protocol that is attached to.
const PROTOCOL = "1.3";

// What `promise` settles with, or a failure if it has not settled within `wait`
// milliseconds.
async function bounded(promise, wait = BROWSER_WAIT_MS) {
  let timer;
  const stuck = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Failure("failed")), wait);
  });
  try {
    return await Promise.race([promise, stuck]);
  } finally {
    clearTimeout(timer);
  }
}

// How long sendWatching() waits for a dialog to open after it sends input, in
// milliseconds.
export const DIALOG_WAIT_MS = 2_000;

// How long the browser gets to answer a read, a list of its tabs, an attach or a
// detach, in milliseconds. A page that holds the browser's answer for ever would
// otherwise hold the request, and with it the one place a request may be answered
// in. Input is not held to this: a dialog the input opens keeps the browser from
// answering it, and sendWatching() and the tool's own wait handle that.
export const BROWSER_WAIT_MS = 15_000;

// How long the end of a call has, in milliseconds, to send the release it still
// owes (RELEASE_WAIT_MS at most) and to detach, which together wait no longer than
// this for the browser. The call has said all it will say, so a browser that has
// stopped answering must not hold the reply for the wait of a read.
export const DETACH_WAIT_MS = 2_000;

// How long the end of a call, or a call that is releasing what an input that
// failed may have pressed, waits for the browser to answer the release, in
// milliseconds. The release is sent at once and its failure is dropped, so the
// wait only holds the call.
export const RELEASE_WAIT_MS = 1_000;

// How long a call has before the person is first asked, in milliseconds, to look
// for the tab, attach, read the page and find the element. Nothing has been done
// to the page when it passes, and the call ends with a sentence that says so.
export const BEFORE_WINDOW_MS = 5_000;

// How long a call has after the person's answer to finish, in milliseconds,
// whatever it is doing: finding the element again, attaching, sending input,
// reading. The person's answer is the Allow of the first window or, for a dialog
// the page opened, the answer given in the panel. BEFORE_WINDOW_MS, the 60
// seconds of the person (approval.js CONFIRMATION_WAIT_MS), the closing of the
// window (confirmation.js CLOSE_WAIT_MS), these seconds and DETACH_WAIT_MS fit in
// the 80 seconds the BraveBot extension waits (extension/trusted.js WAIT_MS) with
// 1 second to spare. It is longer than the 10 seconds that an input the browser
// stops answering takes to end a call, with its release (tools.js), and than the
// wait for a picture (capture.js) and for the console (consoles.js), so those
// waits are not cut short.
export const CALL_WAIT_MS = 11_000;

// The event the browser sends when a page opens a dialog.
const DIALOG_EVENT = "Page.javascriptDialogOpening";

// The events the browser sends for what a page writes to its console
// (Console.messageAdded, for the calls of console and for the errors a script
// throws) and for what the browser itself reports about the page (Log.entryAdded,
// for a request that failed, a rule of the page that was broken, an API that is
// going away). Both are collected only while readConsole() runs.
const CONSOLE_EVENT = "Console.messageAdded";
const LOG_EVENT = "Log.entryAdded";

// How long readConsole() waits after the browser has answered Log.enable, in
// milliseconds. The browser sends what it collected so far as it enables each
// domain, and this is the time left for what arrives with it.
export const CONSOLE_SETTLE_MS = 500;

// The methods of the protocol this extension sends, each with how it is sent:
// false for a read, true for input, which needs a token the session's approval
// accepts, "dialog" for the answer to a dialog, which is sent with the person's
// answer and nothing else, and "file" for the choice of a file on a file input,
// which needs a token like input and is sent only with one file whose path
// files.js accepts and the node of the input. Input is not sent before
// Page.enable, since only a session that had it enabled when a dialog opened can
// answer the dialog.
// Page.captureScreenshot is the one method that reads the page's pixels, and it
// needs a token like input: it reads parts nobody sees. Console.enable and
// Log.enable start the browser's report of what the page wrote to its console,
// and they are sent with a token like input is: they evaluate nothing, and the
// page's console can hold anything its scripts printed. The Console domain is
// the one that reports the calls of console.log and the others, and the Log
// domain does not. The list holds no Runtime method, so nothing, not even a fixed
// function, runs in a page, and none of Network, Storage, Fetch, Target, Browser
// or Emulation, nor Page.navigate, Page.printToPDF or Input.dispatchTouchEvent.
export const ALLOWED = Object.freeze({
  "Accessibility.enable": false,
  "Accessibility.getFullAXTree": false,
  "CSS.enable": false,
  "CSS.getComputedStyleForNode": false,
  "DOM.enable": false,
  "DOM.getBoxModel": false,
  "DOM.getFlattenedDocument": false,
  "DOM.getNodeForLocation": false,
  "DOM.scrollIntoViewIfNeeded": false,
  "Page.bringToFront": false,
  "Page.enable": false,
  "Page.getFrameTree": false,
  "Page.getLayoutMetrics": false,
  "Console.enable": true,
  "DOM.focus": true,
  "DOM.setFileInputFiles": "file",
  "Input.dispatchKeyEvent": true,
  "Input.dispatchMouseEvent": true,
  "Input.insertText": true,
  "Log.enable": true,
  "Page.captureScreenshot": true,
  "Page.handleJavaScriptDialog": "dialog",
});

// What an open session holds, by session: the debugger API and the approval,
// whether Page is enabled, the dialog the page has open (or null), what to call
// when one opens, and what to call with each entry of the console while
// readConsole() runs (or null). A session value carries only the tab's number, so code
// that holds one cannot reach the API except through command(), and a session
// that has ended is no longer here.
const open = new WeakMap();

// The text of a field of an event, or "" when it is not text.
const textOf = (value) => (typeof value === "string" ? value : "");

// Whether `value` is the address of a page on the web.
export function isWebUrl(value) {
  try {
    return ["http:", "https:"].includes(new URL(value).protocol);
  } catch {
    return false;
  }
}

// The browser's debugger API of `chrome`.
const debuggerOf = (chrome) => chrome.debugger;

// The cleaning up that the worker does as it starts, or null before it does, and
// withSession() waits for it before it attaches: a call that comes with the start
// of the worker must not attach to a tab that cleaning up then detaches.
let leftovers = null;

// Detaches the debugger from every tab it is attached to as this worker starts. A
// session lasts for one call, so one that is still attached belongs to a worker
// that stopped, and the browser keeps it, so that no call could attach to the tab
// until the person closed the debugging bar. A tab that another debugger holds,
// such as the browser's developer tools, is not this extension's, the detach
// fails, and nothing is changed. Answers when it is done, and never throws.
export function detachLeftovers(chrome) {
  const api = debuggerOf(chrome);
  leftovers = (async () => {
    const targets = await bounded(api.getTargets());
    await Promise.all(
      targets
        .filter((target) => target.type === "page" && Number.isInteger(target.tabId) && target.attached === true)
        .map((target) => bounded(api.detach({ tabId: target.tabId }), DETACH_WAIT_MS).catch(() => {})),
    );
  })().catch(() => {});
  return leftovers;
}

// The number of the one tab at exactly `url`, character for character. None or
// several is a failure that says how many and names no tab.
async function tabAt(api, url) {
  const targets = await bounded(api.getTargets());
  const tabs = targets.filter(
    (target) =>
      target.type === "page" &&
      Number.isInteger(target.tabId) &&
      target.url === url,
  );
  if (tabs.length !== 1) {
    throw new Failure("tabs", tabs.length);
  }
  return tabs[0].tabId;
}

// The time of a call. begin() counts BEFORE_WINDOW_MS, for the steps before the
// person is first asked, and start() counts CALL_WAIT_MS, for the steps after
// their answer. Each counts again from the beginning when it is called, and
// replaces the other, until stop(). Nothing counts before the first begin() or
// start() or while it is stopped, as while the person is asked, and while they
// answer a dialog in the panel. within(promise) settles as `promise` does, or with
// the failure "slow" if the time before the window passes first, or "late" if the
// time after the answer does, and what a promise abandoned that way later does is
// not reported.
export function callTime({ before = BEFORE_WINDOW_MS, after = CALL_WAIT_MS } = {}) {
  let timer = null;
  let pass;
  const passed = new Promise((_, reject) => {
    pass = (code) => reject(new Failure(code));
  });
  passed.catch(() => {});
  const stop = () => {
    clearTimeout(timer);
    timer = null;
  };
  const count = (wait, code) => {
    stop();
    timer = setTimeout(pass, wait, code);
  };
  return Object.freeze({
    begin: () => count(before, "slow"),
    start: () => count(after, "late"),
    stop,
    within(promise) {
      promise.catch(() => {});
      return Promise.race([promise, passed]);
    },
  });
}

// Runs `work` with a session on the one tab at `url`, and returns what it
// returns. `allows(token)` says whether a token was made by the person's
// approval; with none given, nothing is allowed and no method that sends input
// is sent. `time` is the call's callTime(). When it passes, the call ends with
// the failure "slow" or "late" wherever it is: looking for the tab, attaching or in
// `work`. The session ends at that moment, so `work`, if it goes on, sends nothing
// more. However the call ends, the input it still owes (owe() below) is sent once,
// unless the page has a dialog open, and the session is detached, within
// DETACH_WAIT_MS together, and a failure of either does not replace the error that
// ended the work. An attach that is answered after the call gave up on it is
// detached.
export async function withSession(
  chrome,
  url,
  work,
  { allows = () => false, time = callTime() } = {},
) {
  if (!isWebUrl(url)) {
    throw new Refusal("argument", "url");
  }
  const api = debuggerOf(chrome);
  let tabId;
  try {
    if (leftovers !== null) {
      await time.within(leftovers);
    }
    tabId = await time.within(tabAt(api, url));
    const attaching = api.attach({ tabId }, PROTOCOL);
    try {
      await time.within(bounded(attaching));
    } catch (error) {
      attaching.then(() => api.detach({ tabId })).catch(() => {});
      throw error;
    }
  } catch (error) {
    time.stop();
    throw error;
  }
  const session = Object.freeze({ tabId });
  const held = { api, allows, enabled: false, dialog: null, wake: null, console: null, owed: null };
  // Only what the event says of the dialog's kind, words and suggested answer
  // is read. The dialog is the page's, so these are the page's words. An entry of
  // the console is handed on whole to whoever asked for them, with the domain it
  // came from, and read by nothing here.
  const watch = (source, method, params) => {
    if (source?.tabId !== tabId) {
      return;
    }
    if (method === DIALOG_EVENT) {
      held.dialog = Object.freeze({
        type: textOf(params?.type),
        message: textOf(params?.message),
        defaultPrompt: textOf(params?.defaultPrompt),
      });
      held.wake?.();
    } else if (method === CONSOLE_EVENT) {
      held.console?.("console", params?.message);
    } else if (method === LOG_EVENT) {
      held.console?.("log", params?.entry);
    }
  };
  open.set(session, held);
  try {
    api.onEvent.addListener(watch);
    return await time.within(work(session));
  } finally {
    time.stop();
    open.delete(session);
    try {
      api.onEvent.removeListener(watch);
    } catch {
      // Nothing is listening any more.
    }
    try {
      await bounded(finish(session, held), DETACH_WAIT_MS);
    } catch {
      // The tab closed or the browser detached first, or did not answer, and
      // there is nothing left to do.
    }
  }
}

// The end of a call: the release it owes, if any, and the detach. A page with a
// dialog open gets no input, as in sendWatching(), since the browser answers none
// and the release would be applied when the person answers the dialog. The
// release is bounded by RELEASE_WAIT_MS and its failure is dropped.
async function finish(session, held) {
  const owed = held.owed;
  held.owed = null;
  if (owed !== null && held.dialog === null) {
    try {
      await bounded(send(session, held, owed.method, owed.params, owed.token), RELEASE_WAIT_MS);
    } catch {
      // The browser did not take it, and the call has no time left to ask again.
    }
  }
  await held.api.detach({ tabId: session.tabId });
}

// Records the input that undoes the input a call is about to send, the release of
// a button or a key. If the call ends with it still recorded, withSession() sends
// it once, before it detaches, so a call that runs out of time does not leave a
// button or a key held. It is checked as any input is: on the list, enabled, and
// with a token the approval accepts. A later call replaces it.
export function owe(session, method, params, token) {
  const held = open.get(session);
  if (held === undefined) {
    throw new Failure("ended");
  }
  if (!Object.hasOwn(ALLOWED, method) || ALLOWED[method] !== true) {
    throw new Refusal("method");
  }
  if (held.allows(token) !== true) {
    throw new Refusal("not_allowed");
  }
  held.owed = Object.freeze({ method, params, token });
}

// Forgets what owe() recorded, because the input was sent and answered, or is
// being sent by the call itself, or must not be sent.
export function settle(session) {
  const held = open.get(session);
  if (held !== undefined) {
    held.owed = null;
  }
}

// The parameters of the answer to a dialog, from the person's answer: the
// accept flag, and for a prompt the text they typed. The answer is
// {resolution, token, accept, text}, made for the dialog this session has open
// (resolution.dialog is that dialog) with a token the dialog panel's window
// gave. Anything else is refused, and so is a token that was made for another
// question or has been used.
function answerParams(held, answer) {
  const { resolution, token, accept, text } = answer ?? {};
  const dialog = held.dialog;
  if (
    dialog === null ||
    resolution?.dialog !== dialog ||
    typeof accept !== "boolean" ||
    (accept && dialog.type === "prompt" && !(typeof text === "string" && [...text].length <= TYPED_TEXT_LIMIT)) ||
    !spend(token, resolution)
  ) {
    throw new Refusal("not_allowed");
  }
  if (!accept) {
    return { accept: false };
  }
  return dialog.type === "prompt" ? { accept: true, promptText: text } : { accept: true };
}

// Whether `params` is the choice of one file on one node: the one file's path is
// accepted by files.js, with its size and name taken from the path itself, and
// the node is a backend node number. Nothing else is in it, so the browser is not
// given a second file, a node of another kind or a parameter this code never read.
function isFileChoice(params) {
  const keys = Object.keys(params ?? {}).sort().join();
  if (keys !== "backendNodeId,files" || !Number.isSafeInteger(params.backendNodeId)) {
    return false;
  }
  const { files } = params;
  if (!Array.isArray(files) || files.length !== 1 || typeof files[0] !== "string") {
    return false;
  }
  const name = files[0].slice(files[0].lastIndexOf("/") + 1);
  return problemWithFile({ path: files[0], size: 0, file_name: name }) === null;
}

// Sends one method to the session's tab and returns the browser's answer, which
// is the page's data and is for the caller to read and not to pass on. A method
// that is not on the list is refused, and so is one that sends input without a
// token the approval accepts, and a dialog answer that is not the person's.
// Nothing reaches the browser in any of these cases. `params` is not read for
// the answer to a dialog, whose parameters come from the answer in `token`.
export async function command(session, method, params, token) {
  const held = open.get(session);
  if (held === undefined) {
    throw new Failure("ended");
  }
  return send(session, held, method, params, token);
}

// The checks of command() and the one use of the browser's sendCommand. It is
// also how the end of a call sends the release the call owes, after the session
// has ended and command() refuses.
async function send(session, held, method, params, token) {
  if (!Object.hasOwn(ALLOWED, method)) {
    throw new Refusal("method");
  }
  const kind = ALLOWED[method];
  let sent = params;
  if ((kind === true || kind === "file") && held.allows(token) !== true) {
    throw new Refusal("not_allowed");
  }
  if (kind === "file" && !isFileChoice(params)) {
    throw new Refusal("argument", "path");
  }
  if (kind === "dialog") {
    sent = answerParams(held, token);
  }
  if (kind !== false && !held.enabled) {
    throw new Failure("failed");
  }
  const sending = held.api.sendCommand({ tabId: session.tabId }, method, sent);
  const answer = await (kind === false ? bounded(sending) : sending);
  if (method === "Page.enable") {
    held.enabled = true;
  }
  if (kind === "dialog") {
    held.dialog = null;
  }
  return answer;
}

// The dialog the page has open in this session, as {type, message,
// defaultPrompt}, or null. The words are the page's.
export function dialogOf(session) {
  return open.get(session)?.dialog ?? null;
}

// Sends input that may open a dialog. A dialog stops the browser from answering
// the input that opened it, so this waits only until the browser answers, a
// dialog opens or `wait` milliseconds have passed, and does not wait for the
// answer past that. It answers {dialog, done}: the dialog that is open, or null,
// and the promise of the browser's answer, which a dialog keeps from settling
// until it is answered. A session with a dialog open sends nothing.
export async function sendWatching(session, method, params, token, wait = DIALOG_WAIT_MS) {
  const held = open.get(session);
  if (held === undefined) {
    throw new Failure("ended");
  }
  if (held.dialog !== null) {
    throw new Failure("dialog_open");
  }
  const opened = new Promise((resolve) => {
    held.wake = resolve;
  });
  const done = command(session, method, params, token);
  // An error after this stopped waiting belongs to whoever awaits `done`.
  done.catch(() => {});
  let timer;
  const late = new Promise((resolve) => {
    timer = setTimeout(resolve, wait);
  });
  try {
    await Promise.race([done, opened, late]);
  } finally {
    clearTimeout(timer);
    held.wake = null;
  }
  return Object.freeze({ dialog: held.dialog, done });
}

// Answers the dialog the page has open with the person's answer.
export function handleDialog(session, answer) {
  return command(session, "Page.handleJavaScriptDialog", undefined, answer);
}

// Has the browser report the page's console and what it reports about the page,
// and calls `onEntry(domain, entry)` with each entry as the browser gave it: the
// domain is "console" or "log", and the entry is the page's data, for the caller
// to read and not to pass on. The browser sends what it collected so far as it
// enables each domain, and this waits `wait` milliseconds after the second answer
// for what comes with it, and then stops. Both methods need the person's `token`.
// Nothing is reported once this returns, or throws.
export async function readConsole(session, token, onEntry, wait = CONSOLE_SETTLE_MS) {
  const held = open.get(session);
  if (held === undefined) {
    throw new Failure("ended");
  }
  held.console = onEntry;
  let timer;
  try {
    await command(session, "Console.enable", undefined, token);
    await command(session, "Log.enable", undefined, token);
    await new Promise((resolve) => {
      timer = setTimeout(resolve, wait);
    });
  } finally {
    clearTimeout(timer);
    held.console = null;
  }
}

// Fails unless exactly one tab is at `url`, as withSession() does when it looks
// for the tab, and attaches to nothing. A tool uses it to find out before it asks
// the person.
export async function requireTab(chrome, url) {
  if (!isWebUrl(url)) {
    throw new Refusal("argument", "url");
  }
  await tabAt(debuggerOf(chrome), url);
}

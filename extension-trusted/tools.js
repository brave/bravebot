// The five tools that act on an element, and the table of all seven handlers (the
// picture, screenshot_full_page, is capture.js and the console, read_console, is
// consoles.js): what each does for a request that requests.js has checked. A tool
// finds the element or elements its arguments name, shows them to the person in
// the confirmation window, and sends the input only after the person allows it.
// It finds them again after the window and acts only when they are the ones
// shown, and just before it presses or releases it asks the browser again what is
// at the point it presses.
//
// Nothing here is sent before the token of the person's Allow: the session's
// approval accepts no token until the window has made one for this call, and
// cdp.js sends no input without it. A tool answers a status word and the origin
// of the address, and nothing of the page. Whatever goes wrong is an error of
// sentences.js, and the session is detached however the call ends. From the
// call's start the steps before the first window have BEFORE_WINDOW_MS, and from
// the person's answer a call has CALL_WAIT_MS (cdp.js) to end, whatever the
// browser is doing. A call that ends with a button or key still down releases it.

import { describe } from "./actions.js";
import { confirm, spend } from "./approval.js";
import { takePicture } from "./capture.js";
import {
  RELEASE_WAIT_MS,
  callTime,
  command,
  handleDialog,
  owe as oweInput,
  sendWatching,
  settle as settleInput,
  withSession,
} from "./cdp.js";
import { readConsoleOf } from "./consoles.js";
import { fileWords, problemWithFile } from "./files.js";
import {
  requireFocus,
  requireReach,
  requireSameElement,
  resolveElement,
  resolveFileInput,
  resolveFocused,
} from "./resolve.js";
import { Failure, Refusal } from "./sentences.js";
import { askDialog } from "./windows.js";

// How long a call waits for the browser to answer an input, after the wait for
// a dialog (cdp.js DIALOG_WAIT_MS) and with no dialog open, in milliseconds. An
// input the browser does not answer by then ends the call.
export const INPUT_WAIT_MS = 5_000;

// The release that follows an input that failed (cdp.js RELEASE_WAIT_MS) adds
// DIALOG_WAIT_MS + RELEASE_WAIT_MS to a call, after the DIALOG_WAIT_MS +
// INPUT_WAIT_MS of the input that failed.

// How many moves a drag makes from where it presses to where it releases, the
// last of them at the release.
export const DRAG_MOVES = 8;

// The keys of words.js KEYS as the browser is sent them. Windows virtual key
// codes. Enter also sends the character, which is what activates a button and
// submits a form. Each key is sent alone, with no modifier.
export const KEY_EVENTS = Object.freeze({
  Enter: Object.freeze({ key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" }),
  Escape: Object.freeze({ key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 }),
  Tab: Object.freeze({ key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 }),
  ArrowUp: Object.freeze({ key: "ArrowUp", code: "ArrowUp", windowsVirtualKeyCode: 38 }),
  ArrowDown: Object.freeze({ key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 }),
  ArrowLeft: Object.freeze({ key: "ArrowLeft", code: "ArrowLeft", windowsVirtualKeyCode: 37 }),
  ArrowRight: Object.freeze({ key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 }),
  Home: Object.freeze({ key: "Home", code: "Home", windowsVirtualKeyCode: 36 }),
  End: Object.freeze({ key: "End", code: "End", windowsVirtualKeyCode: 35 }),
  PageUp: Object.freeze({ key: "PageUp", code: "PageUp", windowsVirtualKeyCode: 33 }),
  PageDown: Object.freeze({ key: "PageDown", code: "PageDown", windowsVirtualKeyCode: 34 }),
});

// The approval of one call. It accepts nothing until the call has the
// resolution the window is asked about, and then only the token the window made
// for that resolution. A token is spent by the first check, so the first
// command that sends input keeps it, and the later commands of the same action
// have to present that same token. ask() is the one place a call asks the person:
// it records the resolution, answers the token of the person's Allow and counts
// `time`, the call's callTime(), which withSession() holds the call to. The time
// before the window counts from the start of the call, runs out while the person
// is asked and counts the time after the answer from the Allow. A denial and a
// window that gives no token are a refusal.
export function permission() {
  const held = { resolution: null, token: null };
  const time = callTime();
  time.begin();
  return {
    held,
    time,
    async ask(resolution) {
      held.resolution = resolution;
      time.stop();
      const token = await confirm(resolution);
      if (typeof token !== "object" || token === null) {
        throw new Refusal("denied");
      }
      time.start();
      return token;
    },
    allows(token) {
      if (held.token !== null) {
        return token === held.token;
      }
      if (held.resolution !== null && spend(token, held.resolution)) {
        held.token = token;
        return true;
      }
      return false;
    },
  };
}

// Waits for the browser's answer to an input, for no longer than `wait`.
async function answered(done, wait) {
  let timer;
  const stuck = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Failure("failed")), wait);
  });
  try {
    await Promise.race([done, stuck]);
  } finally {
    clearTimeout(timer);
  }
}

// What a call is, as far as its tool is concerned.
//
//   action   (args, found) answers the word the window has for the action
//            (actions.js); found is what find answered
//   find     (session, args) answers the resolutions of the elements, the one
//            acted on first and, for a drag, the one it ends on second
//   words    (args, found) answers what describe() takes besides the page
//   act      ({session, put, owe, settle, found, args}) sends the input; put()
//            sends one input as the call's own and answers a dialog it opens, and
//            waits for the browser's answer for INPUT_WAIT_MS, or for the wait it
//            is given as a third argument; owe(method, params) records the input
//            that undoes the one about to be sent, which the end of the call
//            sends if it is still recorded, and settle() forgets it
//   status   (args) the status word of a call that was done
async function perform(chrome, args, tool, permit) {
  const { url } = args;
  const origin = new URL(url).origin;
  await withSession(
    chrome,
    url,
    async (session) => {
      await command(session, "Page.enable");
      const shown = await tool.find(session, args);
      await command(session, "Page.bringToFront");
      const resolution = Object.freeze({
        description: describe(tool.action(args, shown), {
          origin,
          url,
          ...tool.words(args, shown),
        }),
      });
      const token = await permit.ask(resolution);
      // The page may have changed while the person decided.
      const found = await tool.find(session, args);
      found.forEach((again, at) => requireSameElement(shown[at], again));

      // One input at a time, each with the call's token. A dialog that the input
      // opens is answered by the person in the panel, and the browser's answer to
      // the input follows the person's.
      const put = async (method, params, wait = INPUT_WAIT_MS) => {
        const sent = await sendWatching(session, method, params, token);
        if (sent.dialog !== null) {
          // The panel counts the person's time, and the call's time counts again
          // from the person's answer.
          permit.time.stop();
          const answer = await askDialog(sent.dialog, { origin, url, first: resolution });
          permit.time.start();
          await handleDialog(session, answer);
        }
        await answered(sent.done, wait);
      };
      const owe = (method, params) => oweInput(session, method, params, token);
      const settle = () => settleInput(session);
      await tool.act({ session, put, owe, settle, found, args });
    },
    { allows: permit.allows, time: permit.time },
  );
  return Object.freeze({ status: tool.status(args), origin });
}

// The mouse event for `type` at the point of a resolution.
const mouse = (type, { x, y }, extra = {}) => ({ type, x, y, ...extra });

// The buttons of the mouse as the browser numbers them while one is held.
const HELD = Object.freeze({ left: 1, right: 2 });

// The element a tool of one element acts on.
const only = (found) => found[0];

const CLICK = Object.freeze({
  action: ({ button, count }) =>
    button === "right" ? "right_click" : count === 2 ? "double_click" : "click",
  find: async (session, { url, role, name }) => [await resolveElement(session, { url, role, name })],
  words: (args, found) => ({ element: only(found).description }),
  // From the press on, a click that ends any way but the release that follows
  // it releases the button at the point it pressed at, which is the point the
  // release is sent to. A release that failed is tried once more, and the
  // failure of that try is dropped, so the call ends with the failure that came
  // first. Where the browser answers no input, as while a dialog is open, the
  // release fails. A click that fails before the press releases nothing. A click
  // that the call's time ends in between has the release sent by the end of the
  // call (cdp.js owe()).
  async act({ session, put, owe, settle, found, args }) {
    const element = only(found);
    const { button, count } = args;
    const released = (times) =>
      mouse("mouseReleased", element.point, { button, buttons: 0, clickCount: times });
    const release = (times, wait) => put("Input.dispatchMouseEvent", released(times), wait);
    await put("Input.dispatchMouseEvent", mouse("mouseMoved", element.point));
    for (let times = 1; times <= count; times += 1) {
      // The move can have changed the page, and so can the first click.
      await requireReach(session, element);
      let pressed = false;
      try {
        pressed = true;
        owe("Input.dispatchMouseEvent", released(times));
        await put(
          "Input.dispatchMouseEvent",
          mouse("mousePressed", element.point, { button, buttons: HELD[button], clickCount: times }),
        );
        await release(times);
        pressed = false;
        settle();
      } finally {
        if (pressed) {
          settle();
          await release(times, RELEASE_WAIT_MS).catch(() => {});
        }
      }
    }
  },
  status: ({ button, count }) =>
    button === "right" ? "right clicked" : count === 2 ? "double clicked" : "clicked",
});

// Sends a key down and then its key up, calling `check` before each. From the down
// on, a key whose up was not sent and answered gets the same up once more, however
// the call ends, and the failure of that try is dropped, so the call ends with the
// failure that came first. A down that failed may have been applied, so it is
// released too. Where the browser answers no input, as while a dialog is open, the
// up is refused and not sent. A call that fails before the down, a check included,
// releases nothing. A check that fails between the down and the up sends no up
// either: the up would go to the element that has the focus now, and not to the one
// the down went to. A call that the call's time ends with the key down, a check
// that has not answered included, has the up sent by the end of the call (cdp.js
// owe()), since nothing can be asked then.
async function pressKey({ put, owe, settle }, down, up, check = async () => {}) {
  let pressed = false;
  try {
    await check();
    pressed = true;
    owe("Input.dispatchKeyEvent", up);
    await put("Input.dispatchKeyEvent", down);
    pressed = false;
    await check().catch((error) => {
      settle();
      throw error;
    });
    pressed = true;
    await put("Input.dispatchKeyEvent", up);
    pressed = false;
    settle();
  } finally {
    if (pressed) {
      settle();
      await put("Input.dispatchKeyEvent", up, RELEASE_WAIT_MS).catch(() => {});
    }
  }
}

const TYPE = Object.freeze({
  action: () => "type",
  async find(session, { url, role, name }) {
    const element = await resolveElement(session, { url, role, name });
    if (!element.editable) {
      throw new Failure("unsupported");
    }
    return [element];
  },
  words: ({ text }, found) => ({ element: only(found).description, text }),
  async act({ session, put, owe, settle, found, args }) {
    const element = only(found);
    await put("DOM.focus", { backendNodeId: element.backendNodeId });
    // The text goes to the element that has the focus, so it has to be this one.
    const focused = await resolveFocused(session, { url: args.url });
    if (focused.backendNodeId !== element.backendNodeId) {
      throw new Failure("changed");
    }
    // The text replaces what the field holds. The browser runs the command that
    // selects all of it, and the page sees a key down and up of no key it knows,
    // not a press of a letter or of a shortcut. The text itself is one
    // insertText, after the key is up, so there is no key to release if it fails.
    await pressKey(
      { put, owe, settle },
      {
        type: "rawKeyDown",
        key: "Unidentified",
        windowsVirtualKeyCode: 0,
        commands: ["selectAll"],
      },
      { type: "keyUp", key: "Unidentified", windowsVirtualKeyCode: 0 },
    );
    // The text goes to whatever has the focus when it arrives, which a page can
    // have moved since the check above.
    await requireFocus(session, element);
    await put("Input.insertText", { text: args.text });
  },
  status: () => "typed",
});

const PRESS = Object.freeze({
  action: () => "key",
  find: async (session, { url, key }) => [await resolveFocused(session, { url, key })],
  words: ({ key }, found) => ({ element: only(found).description, key }),
  // The key goes to the element that has the focus, which a page can move, so
  // the focus is asked about again just before the key goes down and before it
  // comes up.
  async act({ session, put, owe, settle, found, args }) {
    const event = KEY_EVENTS[args.key];
    await pressKey(
      { put, owe, settle },
      { type: event.text === undefined ? "rawKeyDown" : "keyDown", ...event },
      {
        type: "keyUp",
        key: event.key,
        code: event.code,
        windowsVirtualKeyCode: event.windowsVirtualKeyCode,
      },
      () => requireFocus(session, only(found)),
    );
  },
  status: () => "pressed",
});

const HOVER = Object.freeze({
  action: () => "hover",
  find: async (session, { url, role, name }) => [await resolveElement(session, { url, role, name })],
  words: (args, found) => ({ element: only(found).description }),
  async act({ put, found }) {
    await put("Input.dispatchMouseEvent", mouse("mouseMoved", only(found).point));
  },
  status: () => "hovered",
});

const DRAG = Object.freeze({
  action: () => "drag",
  // The element it ends on is found first, and the one it takes last, so that
  // the page is left scrolled to the one it presses on.
  async find(session, { url, role, name, onto_role: ontoRole, onto_name: ontoName }) {
    const destination = await resolveElement(session, { url, role: ontoRole, name: ontoName });
    const source = await resolveElement(session, { url, role, name });
    return [source, destination];
  },
  words: (args, found) => ({
    element: found[0].description,
    destination: found[1].description,
  }),
  // From the press on, a drag that ends any way but the release at the second
  // element releases the button where it pressed. That is the point the check
  // before the press found the first element at, so the release lands on what
  // the person was shown and not on a point of the path, and not on a cover that
  // now sits over the second element. Where the browser answers no input, as
  // while a dialog is open, the release fails and the call ends with the
  // failure that came first. A drag that the call's time ends in between has the
  // release at the first element sent by the end of the call (cdp.js owe()).
  async act({ session, put, owe, settle, found }) {
    const [source, destination] = found;
    // Both have to be in the window together, as the pointer goes from one to
    // the other.
    await requireReach(session, destination);
    await put("Input.dispatchMouseEvent", mouse("mouseMoved", source.point));
    await requireReach(session, source);
    const back = mouse("mouseReleased", source.point, { button: "left", buttons: 0, clickCount: 1 });
    let pressed = false;
    try {
      pressed = true;
      owe("Input.dispatchMouseEvent", back);
      await put(
        "Input.dispatchMouseEvent",
        mouse("mousePressed", source.point, { button: "left", buttons: 1, clickCount: 1 }),
      );
      for (let move = 1; move <= DRAG_MOVES; move += 1) {
        await put(
          "Input.dispatchMouseEvent",
          mouse(
            "mouseMoved",
            {
              x: Math.round(source.point.x + ((destination.point.x - source.point.x) * move) / DRAG_MOVES),
              y: Math.round(source.point.y + ((destination.point.y - source.point.y) * move) / DRAG_MOVES),
            },
            { button: "left", buttons: 1 },
          ),
        );
      }
      await requireReach(session, destination);
      pressed = false;
      settle();
      await put(
        "Input.dispatchMouseEvent",
        mouse("mouseReleased", destination.point, { button: "left", buttons: 0, clickCount: 1 }),
      );
    } finally {
      if (pressed) {
        settle();
        await put("Input.dispatchMouseEvent", back, RELEASE_WAIT_MS).catch(() => {});
      }
    }
  },
  status: () => "dragged",
});

// A handler of requests.js: runs `tool` with the approval made for the call, which
// counts the time before the window from here, and stops counting however the call
// ends.
const handler = (tool) => async (args, chrome) => {
  const permit = permission();
  try {
    return await tool(args, chrome, permit);
  } finally {
    permit.time.stop();
  }
};

// Chooses a file on a file input. The browser reads the file and gives it to the
// input: no byte of it passes through this extension, and no input is sent, so
// there is nothing to release if the call fails. The file is the one the BraveBot
// extension sent, whose path, size and name the relay's server looked at (files.js
// checks their form and that no part of the path is the name of a credential, and
// the call is refused before the tab is touched if it fails). The window shows the
// input as the page names it and the file's name, size and full path. The page's
// script may send the file on as soon as it is chosen, which the window says. An
// input the page hides is chosen on as well, and the window says that the page
// hides it and whether a label or only its id or name calls it (resolve.js).
const HIDDEN_FILE_ACTIONS = Object.freeze({ label: "upload_hidden", identifier: "upload_hidden_id" });
const FILE = Object.freeze({
  action: (args, found) => HIDDEN_FILE_ACTIONS[only(found).hidden] ?? "upload",
  find: async (session, { url, role, name }) => [await resolveFileInput(session, { url, role, name })],
  words: (args, found) => ({ element: only(found).description, file: fileWords(args) }),
  async act({ put, found, args }) {
    await put("DOM.setFileInputFiles", {
      files: [args.path],
      backendNodeId: only(found).backendNodeId,
    });
  },
  status: () => "chose",
});

// A request to choose a file whose file is not one that may be chosen is refused
// here, before the tab is looked for.
function chooseFile(args, chrome, permit) {
  const problem = problemWithFile(args);
  if (problem === "credential") {
    throw new Refusal("credential");
  }
  if (problem !== null) {
    throw new Refusal("argument", problem);
  }
  return perform(chrome, args, FILE, permit);
}

// The handlers of requests.js, by method. The picture tool is capture.js and the
// console tool is consoles.js; the approval is the one the input tools use, made
// for the call.
export const TOOLS = Object.freeze({
  click_element_trusted: handler((args, chrome, permit) => perform(chrome, args, CLICK, permit)),
  type_text_trusted: handler((args, chrome, permit) => perform(chrome, args, TYPE, permit)),
  press_key_trusted: handler((args, chrome, permit) => perform(chrome, args, PRESS, permit)),
  hover_element: handler((args, chrome, permit) => perform(chrome, args, HOVER, permit)),
  drag_element: handler((args, chrome, permit) => perform(chrome, args, DRAG, permit)),
  screenshot_full_page: handler((args, chrome, permit) => takePicture(chrome, args, permit)),
  read_console: handler((args, chrome, permit) => readConsoleOf(chrome, args, permit)),
  choose_file: handler((args, chrome, permit) => chooseFile(args, chrome, permit)),
});

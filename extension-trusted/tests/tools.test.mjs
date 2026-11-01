// The five tools against a page recorded in a real browser (recorded/world.json)
// and a fake debugger that records every call, in order. The window the person
// answers in is a stand-in that records when it was asked and what it was shown,
// except in the tests of dialogs, which use the real windows over fake pages. The
// tests ask through requests.js, as the BraveBot extension does, so what they
// compare is the reply and the inputs the browser received.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import * as input from "../../extension/input.js";
import { WAIT_MS } from "../../extension/trusted.js";
import { ALLOW_DELAY_MS, CONFIRMATION_WAIT_MS, approve, setConfirmation } from "../approval.js";
import { BEFORE_WINDOW_MS, CALL_WAIT_MS, DETACH_WAIT_MS, DIALOG_WAIT_MS, RELEASE_WAIT_MS } from "../cdp.js";
import { STATUSES, answer } from "../requests.js";
import { Refusal, SENTENCES } from "../sentences.js";
import { DRAG_MOVES, INPUT_WAIT_MS, KEY_EVENTS, permission } from "../tools.js";
import { KEYS } from "../words.js";
import { createWindows, setDialogPanel } from "../windows.js";
import { callsOf, fakeDebugger, pageAt, recordedPage } from "./fake-debugger.mjs";
import { browser, flush } from "./fake-panels.mjs";
import { scripts } from "./scripts.mjs";
import {
  URL_A,
  URL_HIDDEN,
  WORLD,
  addNode,
  backendOf,
  flatOf,
  focusOn,
  fresh,
  hiddenFresh,
  hitOn,
  inputWith,
  middleOf,
  moveNode,
  replaceNode,
  setAttribute,
  showNode,
} from "./world.mjs";

afterEach(() => {
  setConfirmation();
  setDialogPanel();
  mock.timers.reset();
});

const ORIGIN = "https://example.test";

// The methods that send input or move the focus, and the answer to a dialog: the
// calls that change a page.
const ACTS = /^(Input\.|DOM\.focus$|DOM\.setFileInputFiles$|Page\.handleJavaScriptDialog$)/;

// The input the browser received, in order, each as its method and parameters.
const inputsOf = (fake) =>
  callsOf(fake, "sendCommand")
    .filter((entry) => ACTS.test(entry.method))
    .map((entry) => ({ method: entry.method, ...entry.params }));

// The mouse events among them, as "type x,y".
const mouseOf = (fake) =>
  inputsOf(fake)
    .filter((entry) => entry.method === "Input.dispatchMouseEvent")
    .map((entry) => `${entry.type} ${entry.x},${entry.y}`);

const MOUSE = "Input.dispatchMouseEvent";
const KEY = "Input.dispatchKeyEvent";

// A page, a debugger over it and a stand-in for the window. `answers` adds to the
// debugger's answers, `ask(resolution, world)` runs in the window while the person
// decides (to change the page), and `allow` is true to allow, false to answer
// nothing, or a function from the resolution to what the window answers. The tab
// is at `url`, which is URL_A for the page of world.json and URL_HIDDEN for the
// one of hidden-world.json.
function rig({ world = fresh(), url = URL_A, answers = {}, allow = true, ask, failOn } = {}) {
  const page = recordedPage(world);
  const fake = fakeDebugger({
    targets: [pageAt(url, 7)],
    answers: { ...page.answers, ...answers },
    failOn,
  });
  const windows = [];
  setConfirmation(async (resolution) => {
    windows.push({
      description: resolution.description,
      calls: fake.calls.length,
      inputs: inputsOf(fake).length,
      fronted: callsOf(fake, "sendCommand").filter((entry) => entry.method === "Page.bringToFront").length,
    });
    await ask?.(resolution, world);
    const made = typeof allow === "function" ? await allow(resolution) : allow ? approve(resolution) : null;
    return made;
  });
  return { world, page, fake, windows, url };
}

// The reply to a request of `method` for the page, from the BraveBot extension.
const request = (method, params) => ({ v: 1, method, params: { url: URL_A, ...params } });
const call = (r, method, params) => answer(request(method, { url: r.url, ...params }), r.fake.chrome);

const refused = (code, detail) => ({
  v: 1,
  error: { kind: "refusal", message: SENTENCES[code](detail) },
});
const failed = (code, detail) => ({
  v: 1,
  error: { kind: "failed", message: SENTENCES[code](detail) },
});
const done = (status) => ({ v: 1, result: { status, origin: ORIGIN } });

// The middle of the recorded box of the button, link or field `name`.
const middle = (role, name, world = WORLD) =>
  middleOf(world.boxes[backendOf(world, role, name)].border);

const nothingSent = (r) => assert.deepEqual(inputsOf(r.fake), []);

// The file a call of choose_file carries, as the relay's server sends it: the
// canonical path of a file, its size and its name.
const REPORT = Object.freeze({
  path: "/srv/relay/uploads/report.txt",
  size: 12,
  file_name: "report.txt",
});

// A cover over a point: the hit test there now answers the node `over`.
const coverAt = (world, { x, y }, over = 28) => {
  world.hits[`${x},${y}`] = { ...world.hits[`${x},${y}`], backendNodeId: over };
};

test("a click is a move to the middle of the element, then a press and a release of the left button with one click, and the window was asked first", async () => {
  const r = rig();
  const reply = await call(r, "click_element_trusted", { role: "button", name: "Archive" });
  assert.deepEqual(reply, done("clicked"));
  const { x, y } = middle("button", "Archive");
  assert.deepEqual(inputsOf(r.fake), [
    { method: MOUSE, type: "mouseMoved", x, y },
    { method: MOUSE, type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 },
    { method: MOUSE, type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 },
  ]);
  // The person was asked once, about this button, before any input, and the page
  // was brought to the front before that.
  assert.equal(r.windows.length, 1);
  assert.equal(r.windows[0].inputs, 0);
  assert.equal(r.windows[0].fronted, 1);
  assert.deepEqual(
    r.windows[0].description,
    {
      action: "click",
      origin: ORIGIN,
      url: URL_A,
      tag: "button",
      role: "button",
      name: "Archive",
      excerpt: "Archive",
      goes: "",
    },
  );
  assert.equal(callsOf(r.fake, "attach").length, 1);
  assert.equal(callsOf(r.fake, "detach").length, 1);
});

test("a double click presses and releases twice with the click counts 1 and 2, and a right click presses and releases the right button once", async () => {
  const r = rig();
  assert.deepEqual(
    await call(r, "click_element_trusted", { role: "button", name: "Archive", count: 2 }),
    done("double clicked"),
  );
  const { x, y } = middle("button", "Archive");
  assert.deepEqual(inputsOf(r.fake), [
    { method: MOUSE, type: "mouseMoved", x, y },
    { method: MOUSE, type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 },
    { method: MOUSE, type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 },
    { method: MOUSE, type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 2 },
    { method: MOUSE, type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 2 },
  ]);
  assert.equal(r.windows[0].description.action, "double_click");

  const right = rig();
  assert.deepEqual(
    await call(right, "click_element_trusted", { role: "button", name: "Archive", button: "right" }),
    done("right clicked"),
  );
  assert.deepEqual(inputsOf(right.fake), [
    { method: MOUSE, type: "mouseMoved", x, y },
    { method: MOUSE, type: "mousePressed", x, y, button: "right", buttons: 2, clickCount: 1 },
    { method: MOUSE, type: "mouseReleased", x, y, button: "right", buttons: 0, clickCount: 1 },
  ]);
  assert.equal(right.windows[0].description.action, "right_click");
});

test("an element below the fold is clicked at the point in the window, and the check before the press asks the browser for the same point in page coordinates", async () => {
  const r = rig();
  assert.deepEqual(await call(r, "click_element_trusted", { role: "button", name: "Far below" }), done("clicked"));
  const box = middle("button", "Far below");
  assert.ok(r.page.scroll.y > 0, "the page was scrolled to the button");
  const { x, y } = { x: box.x - r.page.scroll.x, y: box.y - r.page.scroll.y };
  assert.deepEqual(mouseOf(r.fake), [`mouseMoved ${x},${y}`, `mousePressed ${x},${y}`, `mouseReleased ${x},${y}`]);
  // Every hit test of the call is at a point of the page, which is the window's
  // point and the scroll.
  const asked = callsOf(r.fake, "sendCommand").filter((entry) => entry.method === "DOM.getNodeForLocation");
  assert.ok(asked.length >= 3, "the resolution, the second resolution and the check before the press");
  for (const entry of asked) {
    assert.deepEqual({ x: entry.params.x, y: entry.params.y }, box);
  }
});

test("no input is sent until the person has allowed it: a denial, a window that answers nothing, a token made for another action and a token used twice each send nothing", async () => {
  const cases = {
    "the person denies": { allow: () => Promise.reject(new Refusal("denied")), reply: refused("denied") },
    "the window answers nothing": { allow: false, reply: refused("denied") },
    "the window answers with something that is not a token": { allow: () => "yes", reply: refused("denied") },
    "the window answers with an object that is not a token": { allow: () => ({}), reply: refused("not_allowed") },
    "the token was made for another resolution": {
      allow: () => approve(Object.freeze({ description: {} })),
      reply: refused("not_allowed"),
    },
  };
  for (const [label, { allow, reply }] of Object.entries(cases)) {
    for (const [method, params] of [
      ["click_element_trusted", { role: "button", name: "Archive" }],
      ["hover_element", { role: "button", name: "Archive" }],
      ["type_text_trusted", { role: "textbox", name: "Full name", text: "x" }],
      ["drag_element", { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" }],
    ]) {
      const r = rig({ allow });
      assert.deepEqual(await call(r, method, params), reply, `${label}: ${method}`);
      nothingSent(r);
      assert.equal(r.windows.length, 1, `${label}: ${method}`);
      assert.equal(callsOf(r.fake, "detach").length, 1, `${label}: ${method}`);
    }
    const key = rig({ allow, world: (() => { const w = fresh(); focusOn(w, 8); return w; })() });
    assert.deepEqual(await call(key, "press_key_trusted", { key: "Enter" }), reply, `${label}: press_key_trusted`);
    nothingSent(key);
  }
  // A token made for the first call serves no other call.
  let old;
  const first = rig({ allow: (resolution) => (old = approve(resolution)) });
  assert.deepEqual(await call(first, "hover_element", { role: "button", name: "Archive" }), done("hovered"));
  const second = rig({ allow: () => old });
  assert.deepEqual(await call(second, "hover_element", { role: "button", name: "Archive" }), refused("not_allowed"));
  nothingSent(second);
});

test("the input is sent only if the element is the one the person was shown: a node that was replaced, moved, renamed or redirected, a page that left its address and an element something now covers each send nothing", async () => {
  const archive = backendOf(WORLD, "button", "Archive");
  const sender = backendOf(WORLD, "button", "Send it");
  const cases = {
    "the node is replaced by another": [(w) => replaceNode(w, archive, 9_999), failed("changed")],
    "the node moves to another parent": [
      (w) => {
        flatOf(w, archive).parentId = flatOf(w, backendOf(w, "button", "Submit")).parentId;
      },
      failed("changed"),
    ],
    "the element is renamed": [
      (w) => {
        w.ax.find((node) => node.backendDOMNodeId === archive).name.value = "Delete";
      },
      failed("none"),
    ],
    "the page address changes": [
      (w) => {
        w.frameTree.frameTree.frame.url = "https://example.test/other.html";
      },
      failed("left"),
    ],
    "something covers the middle": [(w) => coverAt(w, middle("button", "Archive", w)), failed("covered")],
  };
  for (const [label, [change, reply]] of Object.entries(cases)) {
    for (const [method, params] of [
      ["click_element_trusted", { role: "button", name: "Archive" }],
      ["hover_element", { role: "button", name: "Archive" }],
    ]) {
      const r = rig({ ask: (resolution, world) => change(world) });
      assert.deepEqual(await call(r, method, params), reply, `${label}: ${method}`);
      nothingSent(r);
      assert.equal(callsOf(r.fake, "detach").length, 1, label);
    }
  }
  // Where a form goes is part of what the person was shown.
  const redirected = rig({ ask: (resolution, world) => setAttribute(world, sender, "formaction", "/elsewhere") });
  assert.deepEqual(
    await call(redirected, "click_element_trusted", { role: "button", name: "Send it" }),
    failed("changed"),
  );
  nothingSent(redirected);
});

test("a cover that appears after the move stops the press: the mouse was moved, nothing was pressed, and the reply is one sentence", async () => {
  const world = fresh();
  const point = middle("button", "Archive", world);
  const r = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved") {
          coverAt(world, point);
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(r, "click_element_trusted", { role: "button", name: "Archive" }), failed("covered"));
  assert.deepEqual(mouseOf(r.fake), [`mouseMoved ${point.x},${point.y}`]);
  assert.equal(callsOf(r.fake, "detach").length, 1);
});

test("a cover that appears after the first click of a double click stops the second press", async () => {
  const world = fresh();
  const point = middle("button", "Archive", world);
  const r = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseReleased" && params.clickCount === 1) {
          coverAt(world, point);
        }
        return {};
      },
    },
  });
  assert.deepEqual(
    await call(r, "click_element_trusted", { role: "button", name: "Archive", count: 2 }),
    failed("covered"),
  );
  assert.deepEqual(mouseOf(r.fake), [
    `mouseMoved ${point.x},${point.y}`,
    `mousePressed ${point.x},${point.y}`,
    `mouseReleased ${point.x},${point.y}`,
  ]);
});

test("hovering is a move to the middle of the element and nothing else", async () => {
  const r = rig();
  assert.deepEqual(await call(r, "hover_element", { role: "link", name: "Next page" }), done("hovered"));
  const { x, y } = middle("link", "Next page");
  assert.deepEqual(inputsOf(r.fake), [{ method: MOUSE, type: "mouseMoved", x, y }]);
  assert.equal(r.windows[0].description.action, "hover");
  assert.equal(r.windows[0].description.goes, "https://example.test/next?x=1");
});

// A debugger that moves the focus to `backend` when it is asked to, as the
// browser does.
const focusing = (world, backend) => ({
  "DOM.focus": () => {
    focusOn(world, backend);
    return {};
  },
});

test("typing focuses the field, selects all of it by the editing command, and inserts the text, in that order and with no letter key", async () => {
  const world = fresh();
  const field = backendOf(world, "textbox", "Full name");
  const r = rig({ world, answers: focusing(world, field) });
  assert.deepEqual(
    await call(r, "type_text_trusted", { role: "textbox", name: "Full name", text: "Ada <b>Lovelace</b>" }),
    done("typed"),
  );
  assert.deepEqual(inputsOf(r.fake), [
    { method: "DOM.focus", backendNodeId: field },
    { method: KEY, type: "rawKeyDown", key: "Unidentified", windowsVirtualKeyCode: 0, commands: ["selectAll"] },
    { method: KEY, type: "keyUp", key: "Unidentified", windowsVirtualKeyCode: 0 },
    { method: "Input.insertText", text: "Ada <b>Lovelace</b>" },
  ]);
  assert.equal(r.windows[0].inputs, 0);
  assert.equal(r.windows[0].description.action, "type");
  assert.equal(r.windows[0].description.text, "Ada <b>Lovelace</b>");
});

test("a search field is typed into, and an element that is not a text field is not: nothing is asked and nothing is sent", async () => {
  const world = fresh();
  const search = backendOf(world, "searchbox", "Find in page");
  const r = rig({ world, answers: focusing(world, search) });
  assert.deepEqual(await call(r, "type_text_trusted", { role: "textbox", name: "Find in page", text: "x" }), done("typed"));
  for (const [role, name] of [
    ["button", "Archive"],
    ["link", "Next page"],
    ["checkbox", "Accept"],
    ["combobox", "Country"],
    ["menuitem", "Rename"],
  ]) {
    const other = rig();
    assert.deepEqual(await call(other, "type_text_trusted", { role, name, text: "x" }), failed("unsupported"), name);
    assert.equal(other.windows.length, 0, name);
    nothingSent(other);
  }
});

test("a password, payment card and file field, and a disabled or read-only one, are refused by every tool that acts on an element: nothing is asked and nothing is sent", async () => {
  const cases = [
    ["click_element_trusted", { role: "button", name: "Attachment" }, refused("file")],
    ["hover_element", { role: "button", name: "Attachment" }, refused("file")],
    ["type_text_trusted", { role: "textbox", name: "Password", text: "x" }, refused("password")],
    ["type_text_trusted", { role: "textbox", name: "Account secret", text: "x" }, refused("password")],
    ["type_text_trusted", { role: "textbox", name: "Card number", text: "x" }, refused("card")],
    ["type_text_trusted", { role: "textbox", name: "Read only field", text: "x" }, failed("disabled")],
    ["click_element_trusted", { role: "button", name: "Disabled one" }, failed("disabled")],
    ["click_element_trusted", { role: "button", name: "In fieldset" }, failed("disabled")],
    ["drag_element", { role: "button", name: "Archive", onto_role: "textbox", onto_name: "Password" }, refused("password")],
    ["drag_element", { role: "button", name: "Attachment", onto_role: "button", onto_name: "Archive" }, refused("file")],
  ];
  for (const [method, params, reply] of cases) {
    const r = rig();
    assert.deepEqual(await call(r, method, params), reply, `${method} ${params.name}`);
    assert.equal(r.windows.length, 0, `${method} ${params.name}`);
    nothingSent(r);
    assert.equal(callsOf(r.fake, "detach").length, 1);
  }
  for (const [name, reply] of [
    ["Password", refused("password")],
    ["Card number", refused("card")],
    ["Account secret", refused("password")],
  ]) {
    const world = fresh();
    focusOn(world, backendOf(world, "textbox", name));
    const r = rig({ world });
    assert.deepEqual(await call(r, "press_key_trusted", { key: "Enter" }), reply, `a key in ${name}`);
    assert.equal(r.windows.length, 0);
    nothingSent(r);
  }
});

test("typing stops when the focus is not on the field afterwards, as a page can move it, and a field the focus lands on that may not be acted on is refused", async () => {
  const world = fresh();
  const search = backendOf(world, "searchbox", "Find in page");
  const r = rig({ world, answers: focusing(world, search) });
  assert.deepEqual(await call(r, "type_text_trusted", { role: "textbox", name: "Full name", text: "x" }), failed("changed"));
  assert.deepEqual(inputsOf(r.fake), [{ method: "DOM.focus", backendNodeId: backendOf(world, "textbox", "Full name") }]);

  const password = fresh();
  const toPassword = rig({ world: password, answers: focusing(password, backendOf(password, "textbox", "Password")) });
  assert.deepEqual(
    await call(toPassword, "type_text_trusted", { role: "textbox", name: "Full name", text: "x" }),
    refused("password"),
  );
  assert.equal(inputsOf(toPassword.fake).length, 1, "only the focus was moved");

  const nowhere = fresh();
  const toNothing = rig({ world: nowhere, answers: { "DOM.focus": () => { focusOn(nowhere, null); return {}; } } });
  assert.deepEqual(
    await call(toNothing, "type_text_trusted", { role: "textbox", name: "Full name", text: "x" }),
    failed("nofocus"),
  );
  assert.equal(inputsOf(toNothing.fake).length, 1);
});

// The keys as the browser is sent them, written here from the table of Windows
// virtual key codes. Enter also sends the character that goes with the key.
const KEY_CODES = {
  Enter: 13,
  Escape: 27,
  Tab: 9,
  ArrowUp: 38,
  ArrowDown: 40,
  ArrowLeft: 37,
  ArrowRight: 39,
  Home: 36,
  End: 35,
  PageUp: 33,
  PageDown: 34,
};

test("a key is sent down and up on the element that has the focus, alone, with the code the browser takes, and Enter also sends its character", async () => {
  assert.deepEqual(Object.keys(KEY_EVENTS), [...KEYS]);
  assert.deepEqual(Object.keys(KEY_EVENTS), [...input.KEYS]);
  assert.deepEqual(Object.keys(KEY_CODES), [...KEYS]);
  for (const key of KEYS) {
    const world = fresh();
    focusOn(world, backendOf(world, "textbox", "Full name"));
    const r = rig({ world });
    assert.deepEqual(await call(r, "press_key_trusted", { key }), done("pressed"), key);
    const code = KEY_CODES[key];
    assert.deepEqual(
      inputsOf(r.fake),
      [
        {
          method: KEY,
          type: key === "Enter" ? "keyDown" : "rawKeyDown",
          key,
          code: key,
          windowsVirtualKeyCode: code,
          ...(key === "Enter" ? { text: "\r" } : {}),
        },
        { method: KEY, type: "keyUp", key, code: key, windowsVirtualKeyCode: code },
      ],
      key,
    );
    for (const sent of inputsOf(r.fake)) {
      assert.equal(sent.modifiers, undefined);
      assert.equal(sent.commands, undefined);
    }
    assert.equal(r.windows[0].inputs, 0);
  }
});

test("the window for a key shows the element that has the focus and the key, and nothing is asked when no element has the focus", async () => {
  const world = fresh();
  focusOn(world, backendOf(world, "button", "Archive"));
  const r = rig({ world });
  assert.deepEqual(await call(r, "press_key_trusted", { key: "ArrowDown" }), done("pressed"));
  assert.deepEqual(
    r.windows[0].description,
    {
      action: "key",
      origin: ORIGIN,
      url: URL_A,
      tag: "button",
      role: "button",
      name: "Archive",
      excerpt: "Archive",
      goes: "",
      key: "ArrowDown",
    },
  );
  // The focus on the page itself, which is how the browser reports no element.
  const page = rig();
  assert.deepEqual(await call(page, "press_key_trusted", { key: "Enter" }), failed("nofocus"));
  assert.equal(page.windows.length, 0);
  nothingSent(page);
  // Two elements that both say they have the focus are not one.
  const two = fresh();
  focusOn(two, backendOf(two, "button", "Archive"));
  const other = two.ax.find((node) => node.backendDOMNodeId === backendOf(two, "button", "Submit"));
  other.properties.push({ name: "focused", value: { type: "booleanOrUndefined", value: true } });
  const both = rig({ world: two });
  assert.deepEqual(await call(both, "press_key_trusted", { key: "Enter" }), failed("several", 2));
  nothingSent(both);
});

test("a key goes only to the element that was shown: a focus that moved while the person decided sends nothing", async () => {
  const world = fresh();
  focusOn(world, backendOf(world, "button", "Archive"));
  const r = rig({ world, ask: (resolution, page) => focusOn(page, backendOf(page, "button", "Submit")) });
  assert.deepEqual(await call(r, "press_key_trusted", { key: "Enter" }), failed("changed"));
  nothingSent(r);
  const gone = fresh();
  focusOn(gone, backendOf(gone, "button", "Archive"));
  const lost = rig({ world: gone, ask: (resolution, page) => focusOn(page, null) });
  assert.deepEqual(await call(lost, "press_key_trusted", { key: "Enter" }), failed("nofocus"));
  nothingSent(lost);
});

// A page the extension's own move event is the signal for: an empty node inside the
// button when the document is read, which the page's script moves out to the body as
// a layer over the window when the pointer moves, so that the press lands on it.
test("a node that was inside the element when the document was read and is moved out of it by the move before the press stops the press: the check asks the document as it is then", async () => {
  const world = fresh();
  const archive = backendOf(world, "button", "Archive");
  const decoy = addNode(world, archive, "div", ["id", "decoy"]);
  const point = middle("button", "Archive", world);
  const body = world.flat.find((node) => node.localName === "body").backendNodeId;
  let moves = 0;
  const r = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved") {
          moves += 1;
          moveNode(world, decoy, body);
          hitOn(world, point, decoy);
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(r, "click_element_trusted", { role: "button", name: "Archive" }), failed("covered"));
  assert.deepEqual(mouseOf(r.fake), [`mouseMoved ${point.x},${point.y}`]);
  assert.equal(moves, 1);
  assert.equal(callsOf(r.fake, "detach").length, 1);

  // The same node left where it was is a part of the button, and the press goes.
  const staying = fresh();
  const inside = addNode(staying, backendOf(staying, "button", "Archive"), "div", ["id", "decoy"]);
  const quiet = rig({
    world: staying,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved") {
          hitOn(staying, point, inside);
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(quiet, "click_element_trusted", { role: "button", name: "Archive" }), done("clicked"));
});

test("the window shows where the click goes, whichever element it lands on: a link inside the element, a link above it, a button's formaction and the form's action, and says so when it cannot tell", async () => {
  const shownFor = async (edit, method = "click_element_trusted", params = { role: "menuitem", name: "Rename" }) => {
    const world = fresh();
    const rename = backendOf(world, "menuitem", "Rename");
    edit(world, rename, middle("menuitem", "Rename", world));
    const r = rig({ world });
    assert.deepEqual(await call(r, method, params), done(method === "hover_element" ? "hovered" : "clicked"), "the call goes on");
    return r.windows[0].description;
  };
  // A page with no script: a link laid over the element, inside it.
  const nested = await shownFor((world, rename, point) =>
    hitOn(world, point, addNode(world, rename, "a", ["href", "/pay"])),
  );
  assert.equal(nested.goes, "https://example.test/pay");
  assert.equal(nested.goesUnknown, undefined);
  // A link the element is in.
  const above = await shownFor((world, rename) => {
    const row = world.flat.find((node) => node.nodeId === flatOf(world, rename).parentId);
    row.localName = "a";
    row.attributes.push("href", "/pay");
  });
  assert.equal(above.goes, "https://example.test/pay");
  // A button, with its formaction, and the form's action.
  assert.equal((await shownFor(() => {}, "click_element_trusted", { role: "button", name: "Send it" })).goes, "https://example.test/other");
  assert.equal((await shownFor(() => {}, "click_element_trusted", { role: "button", name: "Submit" })).goes, "https://example.test/post");
  // An image map: the address is not in the document.
  const mapped = await shownFor((world, rename) => {
    flatOf(world, rename).localName = "img";
    setAttribute(world, rename, "usemap", "#areas");
  });
  assert.equal(mapped.goes, "");
  assert.equal(mapped.goesUnknown, true);
});

test("a link that the pointer reaches after the person decided stops the press, since the window did not show where it goes", async () => {
  const world = fresh();
  const rename = backendOf(world, "menuitem", "Rename");
  const point = middle("menuitem", "Rename", world);
  const r = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved") {
          hitOn(world, point, addNode(world, rename, "a", ["href", "/pay"]));
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(r, "click_element_trusted", { role: "menuitem", name: "Rename" }), failed("changed"));
  assert.equal(r.windows[0].description.goes, "");
  assert.deepEqual(mouseOf(r.fake), [`mouseMoved ${point.x},${point.y}`]);
});

test("the window for Enter in a text field shows where the form's first submit button sends it, and for another key nowhere", async () => {
  const world = fresh();
  focusOn(world, backendOf(world, "textbox", "Query"));
  const enter = rig({ world });
  assert.deepEqual(await call(enter, "press_key_trusted", { key: "Enter" }), done("pressed"));
  assert.equal(enter.windows[0].description.goes, "https://example.test/other");
  const tab = rig({ world });
  assert.deepEqual(await call(tab, "press_key_trusted", { key: "Tab" }), done("pressed"));
  assert.equal(tab.windows[0].description.goes, "");
});

// A page that moves the focus from the button the person was shown to another
// while the key is on its way: `flipAfter` is the number of the read of the
// accessibility tree after which the focus is on Save. The reads of a key press are
// the first look, the second look after the window, and the look before the key
// goes down and before it comes up; the reads of typing are the first look, the
// second look, the look after the focus is moved and the look before the text.
function flipping(world, flipAfter) {
  const save = backendOf(world, "button", "Save");
  let reads = 0;
  return {
    "Accessibility.getFullAXTree": () => {
      reads += 1;
      const tree = { nodes: structuredClone(world.ax) };
      if (reads === flipAfter) {
        focusOn(world, save);
      }
      return tree;
    },
  };
}

test("a key goes down and up only while the focus is on the element that was shown: a page that moves the focus before the key stops it, and one that moves it between the down and the up gets no up", async () => {
  const keys = (r) => inputsOf(r.fake).filter((entry) => entry.method === KEY).map((entry) => entry.type);
  for (const [label, flipAfter, reply, sent] of [
    ["never", 0, done("pressed"), ["keyDown", "keyUp"]],
    ["after the second look, before the key", 2, failed("changed"), []],
    ["after the look before the key, before the up", 3, failed("changed"), ["keyDown"]],
    ["after the look before the up", 4, done("pressed"), ["keyDown", "keyUp"]],
  ]) {
    const world = fresh();
    focusOn(world, backendOf(world, "button", "Archive"));
    const r = rig({ world, answers: flipping(world, flipAfter) });
    assert.deepEqual(await call(r, "press_key_trusted", { key: "Enter" }), reply, label);
    assert.deepEqual(keys(r), sent, label);
    assert.equal(callsOf(r.fake, "detach").length, 1, label);
  }
});

test("typed text goes to the field only while the focus is on it: a page that moves the focus after the check that follows the focus stops the text, and the keys that selected the text are released", async () => {
  for (const [label, flipAfter, reply, sent] of [
    ["never", 0, done("typed"), ["DOM.focus", KEY, KEY, "Input.insertText"]],
    ["after the look that follows the focus", 3, failed("changed"), ["DOM.focus", KEY, KEY]],
  ]) {
    const world = fresh();
    const r = rig({
      world,
      answers: { ...focusing(world, backendOf(world, "textbox", "Full name")), ...flipping(world, flipAfter) },
    });
    assert.deepEqual(await call(r, "type_text_trusted", { role: "textbox", name: "Full name", text: "hello" }), reply, label);
    assert.deepEqual(inputsOf(r.fake).map((entry) => entry.method), sent, label);
  }
});

test("a drag presses at the middle of the first element, moves in eight steps that end at the middle of the second with the button held, and releases there; the window shows both elements", async () => {
  const r = rig();
  assert.deepEqual(
    await call(r, "drag_element", { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" }),
    done("dragged"),
  );
  const from = middle("button", "Archive");
  const to = middle("button", "Submit");
  assert.equal(DRAG_MOVES, 8);
  const steps = Array.from({ length: 8 }, (_, at) => ({
    method: MOUSE,
    type: "mouseMoved",
    x: Math.round(from.x + ((to.x - from.x) * (at + 1)) / 8),
    y: Math.round(from.y + ((to.y - from.y) * (at + 1)) / 8),
    button: "left",
    buttons: 1,
  }));
  assert.deepEqual(inputsOf(r.fake), [
    { method: MOUSE, type: "mouseMoved", x: from.x, y: from.y },
    { method: MOUSE, type: "mousePressed", x: from.x, y: from.y, button: "left", buttons: 1, clickCount: 1 },
    ...steps,
    { method: MOUSE, type: "mouseReleased", x: to.x, y: to.y, button: "left", buttons: 0, clickCount: 1 },
  ]);
  assert.deepEqual({ x: steps[7].x, y: steps[7].y }, to, "the last step is at the second element");
  assert.notDeepEqual(from, to);
  assert.equal(r.windows.length, 1);
  assert.equal(r.windows[0].inputs, 0);
  const shown = r.windows[0].description;
  assert.equal(shown.action, "drag");
  assert.equal(shown.name, "Archive");
  assert.equal(shown.onto, 'role "button", tag "button", name "Submit", text "Submit", goes to "https://example.test/post"');
});

test("a drag whose second element is not found, is hidden or is out of the window together with the first sends nothing, and asks nothing", async () => {
  const cases = [
    [{ onto_role: "button", onto_name: "Missing" }, failed("none")],
    [{ onto_role: "button", onto_name: "Save" }, failed("several", 2)],
    [{ onto_role: "button", onto_name: "Ghost" }, failed("hidden", 1)],
    [{ onto_role: "button", onto_name: "Covered" }, failed("covered")],
  ];
  for (const [onto, reply] of cases) {
    const r = rig();
    assert.deepEqual(await call(r, "drag_element", { role: "button", name: "Archive", ...onto }), reply, onto.onto_name);
    assert.equal(r.windows.length, 0);
    nothingSent(r);
  }
  // The second element is far below the first, so the page cannot show both.
  const apart = rig();
  const reply = await call(apart, "drag_element", {
    role: "button",
    name: "Archive",
    onto_role: "button",
    onto_name: "Far below",
  });
  assert.equal(reply.error.kind, "failed");
  assert.ok(
    [SENTENCES.offscreen(), SENTENCES.covered()].includes(reply.error.message),
    reply.error.message,
  );
  nothingSent(apart);
});

test("a drag is not made if either element changed while the person decided, and not finished if something covers the second element before the release", async () => {
  const archive = backendOf(WORLD, "button", "Archive");
  const submit = backendOf(WORLD, "button", "Submit");
  for (const [label, change] of [
    ["the first is replaced", (w) => replaceNode(w, archive, 9_999)],
    ["the second is replaced", (w) => replaceNode(w, submit, 9_998)],
    ["the page leaves its address", (w) => { w.frameTree.frameTree.frame.url = "https://example.test/x"; }],
  ]) {
    const r = rig({ ask: (resolution, world) => change(world) });
    const reply = await call(r, "drag_element", { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" });
    assert.equal(reply.error.kind, "failed", label);
    nothingSent(r);
  }
  // The cover comes with the last of the eight moves.
  const world = fresh();
  const to = middle("button", "Submit", world);
  const r = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved" && params.buttons === 1 && params.x === to.x && params.y === to.y) {
          coverAt(world, to);
        }
        return {};
      },
    },
  });
  assert.deepEqual(
    await call(r, "drag_element", { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" }),
    failed("covered"),
  );
  const sent = inputsOf(r.fake);
  assert.equal(sent.length, 11, "the move, the press, the eight steps and the release");
  // The button is released where it was pressed and not on what now covers the
  // second element.
  const from = middle("button", "Archive");
  assert.deepEqual(sent.at(-1), {
    method: MOUSE,
    type: "mouseReleased",
    x: from.x,
    y: from.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  assert.equal(callsOf(r.fake, "detach").length, 1);
});

// A drag that has pressed the button and then fails leaves the button held until
// the next mouse event, so it releases it at the point it pressed at, which the
// check before the press found the first element at. Not at a point of the path
// and not at the second element, where something may now sit.
test("a drag that fails after the press releases the button where it pressed, on every path that can end it: the press, the first, a middle and the last move, a cover over the second element, a browser that stops answering; one that fails before the press releases nothing, and the release at the second element is not sent twice", async () => {
  const drag = { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" };
  const from = middle("button", "Archive");
  const to = middle("button", "Submit");
  const releasedAt = (point) => ({
    method: MOUSE,
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  const whole = rig();
  assert.deepEqual(await call(whole, "drag_element", drag), done("dragged"));
  const normal = inputsOf(whole.fake);
  assert.equal(normal.length, 11);
  assert.deepEqual(normal.at(-1), releasedAt(to));

  // The nth input with the button held (the press is the first) fails, as the
  // browser refuses it, and so does every input after it where `stays` is true.
  const failing = (nth, stays = false) => {
    let held = 0;
    let broken = false;
    return {
      [MOUSE]: (params) => {
        if (params.buttons === 1) {
          held += 1;
          broken ||= held === nth;
        }
        if (broken && (stays || params.buttons === 1)) {
          throw new Error("boom: SECRET");
        }
        return {};
      },
    };
  };
  for (const [label, nth, stays] of [
    ["the press", 1, false],
    ["the first move", 2, false],
    ["a move in the middle", 5, false],
    ["the last move", 9, false],
    ["the press, and the browser then stops answering", 1, true],
    ["a move in the middle, and the browser then stops answering", 5, true],
  ]) {
    const r = rig({ answers: failing(nth, stays) });
    assert.deepEqual(await call(r, "drag_element", drag), failed("failed"), label);
    assert.deepEqual(inputsOf(r.fake), [...normal.slice(0, nth + 1), releasedAt(from)], label);
    assert.equal(callsOf(r.fake, "detach").length, 1, label);
  }

  // Something covers the second element when the last move arrives.
  const world = fresh();
  const covered = rig({
    world,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved" && params.buttons === 1 && params.x === to.x && params.y === to.y) {
          coverAt(world, to);
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(covered, "drag_element", drag), failed("covered"));
  assert.deepEqual(inputsOf(covered.fake), [...normal.slice(0, 10), releasedAt(from)]);

  // The browser then refuses the release too: the call ends with the failure
  // that came first and not with the refusal of the release, which is tried.
  const refusing = fresh();
  const stubborn = rig({
    world: refusing,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved" && params.buttons === 1 && params.x === to.x && params.y === to.y) {
          coverAt(refusing, to);
        }
        if (params.type === "mouseReleased") {
          throw new Error("boom: SECRET");
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(stubborn, "drag_element", drag), failed("covered"));
  assert.deepEqual(inputsOf(stubborn.fake), [...normal.slice(0, 10), releasedAt(from)]);

  // Something covers the first element when the pointer has moved to it: the
  // button was never pressed, and nothing is released.
  const early = fresh();
  const uncovered = rig({
    world: early,
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseMoved" && params.buttons === undefined) {
          coverAt(early, from);
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(uncovered, "drag_element", drag), failed("covered"));
  assert.deepEqual(inputsOf(uncovered.fake), [normal[0]]);

  // The release at the second element is the one that fails: it is not tried
  // again at the first.
  const last = rig({
    answers: {
      [MOUSE]: (params) => {
        if (params.type === "mouseReleased") {
          throw new Error("boom: SECRET");
        }
        return {};
      },
    },
  });
  assert.deepEqual(await call(last, "drag_element", drag), failed("failed"));
  assert.deepEqual(inputsOf(last.fake), normal);
  assert.equal(callsOf(last.fake, "detach").length, 1);
});

// A click that has pressed the button and then fails leaves the button held until
// the next mouse event, so it releases the button at the point it pressed at,
// which is the point of the element the person was shown and the point the
// release of a click is sent to anyway. A hover presses nothing.
test("a click that fails after a press releases the button where it pressed, on every path that can end it: the press or the release of any click of a double click, a right click, a browser that stops answering; one that fails before the press releases nothing, and a hover never releases", async () => {
  const at = middle("button", "Archive");
  const move = { method: MOUSE, type: "mouseMoved", x: at.x, y: at.y };
  const press = (clickCount, button = "left") => ({
    method: MOUSE,
    type: "mousePressed",
    x: at.x,
    y: at.y,
    button,
    buttons: button === "left" ? 1 : 2,
    clickCount,
  });
  const release = (clickCount, button = "left") => ({
    method: MOUSE,
    type: "mouseReleased",
    x: at.x,
    y: at.y,
    button,
    buttons: 0,
    clickCount,
  });

  // The nth press or release fails, as the browser refuses it, and so does each
  // one after it where `stays` is true. A move never fails.
  const failing = (nth, stays = false) => {
    let seen = 0;
    return {
      [MOUSE]: (params) => {
        if (params.type === "mousePressed" || params.type === "mouseReleased") {
          seen += 1;
          if (seen === nth || (stays && seen > nth)) {
            throw new Error("boom: SECRET");
          }
        }
        return {};
      },
    };
  };
  const whole = rig();
  assert.deepEqual(await call(whole, "click_element_trusted", { role: "button", name: "Archive" }), done("clicked"));
  assert.deepEqual(inputsOf(whole.fake), [move, press(1), release(1)], "a click that does not fail is not released twice");

  const click = { role: "button", name: "Archive" };
  const twice = { ...click, count: 2 };
  const right = { ...click, button: "right" };
  for (const [label, args, nth, stays, expected] of [
    ["the press", click, 1, false, [move, press(1), release(1)]],
    ["the release", click, 2, false, [move, press(1), release(1), release(1)]],
    ["the press, and the browser then stops answering", click, 1, true, [move, press(1), release(1)]],
    ["the release, and the browser then stops answering", click, 2, true, [move, press(1), release(1), release(1)]],
    ["the first press of a double click", twice, 1, false, [move, press(1), release(1)]],
    ["the first release of a double click", twice, 2, false, [move, press(1), release(1), release(1)]],
    ["the second press of a double click", twice, 3, false, [move, press(1), release(1), press(2), release(2)]],
    ["the second release of a double click", twice, 4, false, [move, press(1), release(1), press(2), release(2), release(2)]],
    ["the second release of a double click, and the browser then stops answering", twice, 4, true, [move, press(1), release(1), press(2), release(2), release(2)]],
    ["the press of a right click", right, 1, false, [move, press(1, "right"), release(1, "right")]],
    ["the release of a right click", right, 2, false, [move, press(1, "right"), release(1, "right"), release(1, "right")]],
  ]) {
    const r = rig({ answers: failing(nth, stays) });
    assert.deepEqual(await call(r, "click_element_trusted", args), failed("failed"), label);
    assert.deepEqual(inputsOf(r.fake), expected, label);
    assert.equal(callsOf(r.fake, "detach").length, 1, label);
  }

  // Nothing is pressed yet: the move fails, or something covers the element after
  // the move, or after the first click of a double click, which was released.
  const moving = rig({
    answers: {
      [MOUSE]: () => {
        throw new Error("boom: SECRET");
      },
    },
  });
  assert.deepEqual(await call(moving, "click_element_trusted", click), failed("failed"));
  assert.deepEqual(inputsOf(moving.fake), [move]);
  for (const [label, args, trigger, expected] of [
    ["the move", click, (params) => params.type === "mouseMoved", [move]],
    ["the first click", twice, (params) => params.type === "mouseReleased" && params.clickCount === 1, [move, press(1), release(1)]],
  ]) {
    const world = fresh();
    const covering = rig({
      world,
      answers: {
        [MOUSE]: (params) => {
          if (trigger(params)) {
            coverAt(world, at);
          }
          return {};
        },
      },
    });
    assert.deepEqual(await call(covering, "click_element_trusted", args), failed("covered"), label);
    assert.deepEqual(inputsOf(covering.fake), expected, `a cover after ${label}`);
  }

  // A hover is a move and nothing else, on every path.
  const hovering = rig({
    answers: {
      [MOUSE]: () => {
        throw new Error("boom: SECRET");
      },
    },
  });
  assert.deepEqual(await call(hovering, "hover_element", click), failed("failed"));
  assert.deepEqual(inputsOf(hovering.fake), [move]);
  const world = fresh();
  coverAt(world, at);
  const covered = rig({ world });
  assert.deepEqual(await call(covered, "hover_element", click), failed("covered"));
  assert.deepEqual(inputsOf(covered.fake), []);
});

test("a key that is pressed and not released is released: the down or the up of typing and of a key press that fails, with the browser then answering or not; a call that fails before the down or after the up sends no more", async () => {
  const boom = () => {
    throw new Error("boom: SECRET");
  };
  // The nth key event fails as the browser refuses it, and so does each one after
  // it where `stays` is true.
  const failing = (nth, stays = false) => {
    let seen = 0;
    return {
      [KEY]: () => {
        seen += 1;
        if (seen === nth || (stays && seen > nth)) {
          throw new Error("boom: SECRET");
        }
        return {};
      },
    };
  };
  const cases = [
    ["the key down", 1, false],
    ["the key up", 2, false],
    ["the key down, and the browser then stops answering", 1, true],
    ["the key up, and the browser then stops answering", 2, true],
  ];

  // Typing: a select-all key down and up, then the text in one insertText.
  const typing = (answers = {}) => {
    const world = fresh();
    const field = backendOf(world, "textbox", "Full name");
    const r = rig({ world, answers: { ...focusing(world, field), ...answers } });
    const down = { method: KEY, type: "rawKeyDown", key: "Unidentified", windowsVirtualKeyCode: 0, commands: ["selectAll"] };
    const up = { method: KEY, type: "keyUp", key: "Unidentified", windowsVirtualKeyCode: 0 };
    return { r, world, down, up, focus: { method: "DOM.focus", backendNodeId: field }, insert: { method: "Input.insertText", text: "Ada" } };
  };
  const typed = { role: "textbox", name: "Full name", text: "Ada" };
  const whole = typing();
  assert.deepEqual(await call(whole.r, "type_text_trusted", typed), done("typed"));
  assert.deepEqual(inputsOf(whole.r.fake), [whole.focus, whole.down, whole.up, whole.insert], "typing that does not fail releases the key once");
  for (const [label, nth, stays] of cases) {
    const t = typing(failing(nth, stays));
    assert.deepEqual(await call(t.r, "type_text_trusted", typed), failed("failed"), label);
    const expected = nth === 1 ? [t.focus, t.down, t.up] : [t.focus, t.down, t.up, t.up];
    assert.deepEqual(inputsOf(t.r.fake), expected, `typing: ${label}`);
    assert.equal(callsOf(t.r.fake, "detach").length, 1, label);
  }
  // The text is one insertText, sent after the key is up: if it fails the key is
  // not pressed, so nothing more is sent.
  const inserting = typing({ "Input.insertText": boom });
  assert.deepEqual(await call(inserting.r, "type_text_trusted", typed), failed("failed"));
  assert.deepEqual(inputsOf(inserting.r.fake), [inserting.focus, inserting.down, inserting.up, inserting.insert]);
  // Nothing is pressed yet when the focus cannot be moved or lands elsewhere.
  const unfocusable = typing({ "DOM.focus": boom });
  assert.deepEqual(await call(unfocusable.r, "type_text_trusted", typed), failed("failed"));
  assert.deepEqual(inputsOf(unfocusable.r.fake), [unfocusable.focus]);
  const elsewhere = fresh();
  const wrong = rig({ world: elsewhere, answers: focusing(elsewhere, backendOf(elsewhere, "searchbox", "Find in page")) });
  assert.deepEqual(await call(wrong, "type_text_trusted", typed), failed("changed"));
  assert.deepEqual(inputsOf(wrong.fake), [{ method: "DOM.focus", backendNodeId: backendOf(elsewhere, "textbox", "Full name") }]);

  // A key press: a down and an up of the key, with the character for Enter.
  for (const key of ["Enter", "Escape", "Tab"]) {
    const code = KEY_CODES[key];
    const down = {
      method: KEY,
      type: key === "Enter" ? "keyDown" : "rawKeyDown",
      key,
      code: key,
      windowsVirtualKeyCode: code,
      ...(key === "Enter" ? { text: "\r" } : {}),
    };
    const up = { method: KEY, type: "keyUp", key, code: key, windowsVirtualKeyCode: code };
    const focused = (answers) => {
      const world = fresh();
      focusOn(world, backendOf(world, "textbox", "Full name"));
      return rig({ world, answers });
    };
    const clean = focused();
    assert.deepEqual(await call(clean, "press_key_trusted", { key }), done("pressed"), key);
    assert.deepEqual(inputsOf(clean.fake), [down, up], `${key}: a key press that does not fail releases the key once`);
    for (const [label, nth, stays] of cases) {
      const r = focused(failing(nth, stays));
      assert.deepEqual(await call(r, "press_key_trusted", { key }), failed("failed"), `${key}: ${label}`);
      assert.deepEqual(inputsOf(r.fake), nth === 1 ? [down, up] : [down, up, up], `${key}: ${label}`);
      assert.equal(callsOf(r.fake, "detach").length, 1, `${key}: ${label}`);
    }
  }
});

// The calls below run on a fake clock. The person answers the window 59 seconds
// into the 60 they have, and the browser then answers every read and no input
// from the nth input on (the nth input and all after it never settle). The
// answer is the seconds the clock ran from the person's answer to the reply. The
// first extension gives up at WAIT_MS, so the 60 seconds of the person, these
// seconds and a margin for the reply and the detach have to fit in it.
async function secondsAfterAnswer(method, args, { stopsAt, world = fresh(), answers = {} }) {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  let seen = 0;
  const silent = (params) => {
    seen += 1;
    return seen >= stopsAt ? new Promise(() => {}) : {};
  };
  const r = rig({
    world,
    answers: { ...answers, [MOUSE]: silent, [KEY]: silent, "Input.insertText": silent },
    ask: () => new Promise((resolve) => setTimeout(resolve, CONFIRMATION_WAIT_MS - 1_000)),
  });
  let reply = null;
  call(r, args.method, args.params).then((value) => {
    reply = value;
  });
  await flush();
  mock.timers.tick(CONFIRMATION_WAIT_MS - 1_000);
  await flush();
  assert.equal(reply, null, "the call goes on after the person's answer");
  for (let seconds = 1; seconds <= 100; seconds += 1) {
    mock.timers.tick(1_000);
    await flush();
    if (reply !== null) {
      mock.timers.reset();
      return { seconds, reply, r };
    }
  }
  mock.timers.reset();
  return { seconds: null, reply, r };
}

// The wait of a release is short because the release is sent at once and its
// failure is dropped. An input that is not answered ends the call after the wait
// for a dialog and the wait for the answer, and the release sent after it adds
// the wait for a dialog and its own short wait, so a call whose browser stops
// answering ends a fixed time after the person's answer, whichever tool it is.
test("a call whose browser stops answering input ends within 10 seconds of the person's answer, release included, so it replies before the BraveBot extension gives up at 80", async () => {
  const seconds = (milliseconds) => milliseconds / 1000;
  const worst = DIALOG_WAIT_MS + INPUT_WAIT_MS + DIALOG_WAIT_MS + RELEASE_WAIT_MS;
  // A wait for the browser's answer to an input that grows without the others
  // shrinking changes this sum.
  assert.equal(seconds(worst), 10);
  // The 60 seconds of the person, the 10, and 5 for the reply and the detach.
  assert.ok(CONFIRMATION_WAIT_MS + worst + 5_000 <= WAIT_MS, "the bound fits in the wait of the first extension");
  assert.ok(worst < CALL_WAIT_MS, "the deadline of a call cuts a wait that is meant to end the call");

  const click = { role: "button", name: "Archive" };
  const typing = fresh();
  const field = backendOf(typing, "textbox", "Full name");
  const pressing = fresh();
  focusOn(pressing, backendOf(pressing, "textbox", "Full name"));
  const cases = [
    // [label, tool, arguments, the input from which none is answered, seconds, inputs sent]
    ["the press of a click", "click_element_trusted", click, 2, 10, 3],
    ["the release of a click", "click_element_trusted", click, 3, 10, 4],
    ["the second press of a double click", "click_element_trusted", { ...click, count: 2 }, 4, 10, 5],
    ["a key down", "press_key_trusted", { key: "Enter" }, 1, 10, 2],
    ["a key up", "press_key_trusted", { key: "Enter" }, 2, 10, 3],
    ["the select-all key of typing", "type_text_trusted", { role: "textbox", name: "Full name", text: "Ada" }, 1, 10, 3],
    ["the text of typing, which has no key to release", "type_text_trusted", { role: "textbox", name: "Full name", text: "Ada" }, 3, 7, 4],
    ["a move of a drag", "drag_element", { ...click, onto_role: "button", onto_name: "Submit" }, 4, 10, 5],
    ["a hover, which presses nothing", "hover_element", click, 1, 7, 1],
  ];
  for (const [label, method, params, stopsAt, expected, sent] of cases) {
    const world = method === "type_text_trusted" ? typing : method === "press_key_trusted" ? pressing : fresh();
    const answers = method === "type_text_trusted" ? focusing(world, field) : {};
    const { seconds: took, reply, r } = await secondsAfterAnswer(method, { method, params }, { stopsAt, world, answers });
    assert.deepEqual(reply, failed("failed"), label);
    assert.equal(took, expected, `${label}: seconds after the answer`);
    assert.ok(CONFIRMATION_WAIT_MS / 1000 - 1 + took + 5 <= WAIT_MS / 1000, `${label}: the reply is early enough`);
    assert.equal(inputsOf(r.fake).length, sent, `${label}: inputs sent`);
    assert.equal(callsOf(r.fake, "detach").length, 1, label);
  }
});

// The calls below run on a fake clock. The person answers the window 59 seconds
// into the 60 they have. From then on the browser's calls are counted: the list
// of tabs, an attach, each command and the detach. From the nth on, the browser
// does what it is sent, as a browser does, and answers `delay` milliseconds
// later, or never when `delay` is Infinity. The answer is the seconds the clock
// ran from the person's answer to the reply, the reply, how many calls the
// browser had, and the inputs it had received when the reply came and 60 seconds
// later.
async function afterTheAnswer(method, params, { from = Infinity, delay = Infinity, world = fresh(), answers = {} } = {}) {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const person = { answered: false, calls: 0, sequence: [] };
  const r = rig({
    world,
    answers,
    ask: () =>
      new Promise((resolve) =>
        setTimeout(() => {
          person.answered = true;
          resolve();
        }, CONFIRMATION_WAIT_MS - 1_000),
      ),
  });
  const api = r.fake.chrome.debugger;
  for (const name of ["getTargets", "attach", "sendCommand", "detach"]) {
    const original = api[name];
    api[name] = (...args) => {
      const settled = original(...args);
      settled.catch(() => {});
      if (!person.answered) {
        return settled;
      }
      person.calls += 1;
      person.sequence.push(name === "sendCommand" ? args[1] : name);
      if (person.calls < from) {
        return settled;
      }
      return delay === Infinity
        ? new Promise(() => {})
        : new Promise((resolve, reject) => setTimeout(() => settled.then(resolve, reject), delay));
    };
  }
  let reply = null;
  call(r, method, params).then((value) => {
    reply = value;
  });
  await flush();
  mock.timers.tick(CONFIRMATION_WAIT_MS - 1_000);
  await flush();
  let seconds = null;
  for (let at = 0; at <= 100 && seconds === null; at += 1) {
    if (reply !== null) {
      seconds = at;
    } else {
      mock.timers.tick(1_000);
      await flush();
    }
  }
  const atReply = inputsOf(r.fake).length;
  for (let at = 0; at < 60; at += 1) {
    mock.timers.tick(1_000);
    await flush();
  }
  const later = inputsOf(r.fake).length;
  mock.timers.reset();
  return { seconds, reply, r, calls: person.calls, sequence: person.sequence, atReply, later };
}

// Each tool, as a call the page lets through: the method, its arguments, the page
// and the browser's extra answers. A call changes its page, so each is made when
// it is wanted.
const PICTURE = Buffer.from([0xff, 0xd8, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0, 100, 0, 100, 1, 1, 0x11, 0, 0xff, 0xd9]).toString("base64");
const click = { role: "button", name: "Archive" };
const EACH_TOOL = [
  () => ["click_element_trusted", click, fresh(), {}],
  () => {
    const world = fresh();
    const field = backendOf(world, "textbox", "Full name");
    return ["type_text_trusted", { role: "textbox", name: "Full name", text: "Ada" }, world, focusing(world, field)];
  },
  () => {
    const world = fresh();
    focusOn(world, backendOf(world, "textbox", "Full name"));
    return ["press_key_trusted", { key: "Enter" }, world, {}];
  },
  () => ["hover_element", click, fresh(), {}],
  () => ["drag_element", { ...click, onto_role: "button", onto_name: "Submit" }, fresh(), {}],
  () => ["screenshot_full_page", {}, fresh(), { "Page.captureScreenshot": () => ({ data: PICTURE }) }],
  () => ["read_console", {}, fresh(), {}],
  () => ["choose_file", { ...REPORT, name: "Attachment" }, fresh(), {}],
];
const withTool = (tool, options) => {
  const [method, params, world, answers] = tool();
  return afterTheAnswer(method, params, { ...options, world, answers });
};

test("a call ends within 11 seconds of the person's answer and its detach within 2 more, whichever tool it is and whichever step the browser stops answering at: the tab list, the attach, a read, an input or the detach", async () => {
  assert.ok(BEFORE_WINDOW_MS + CONFIRMATION_WAIT_MS + CALL_WAIT_MS + DETACH_WAIT_MS <= WAIT_MS, "the bound fits in the wait of the first extension");
  assert.deepEqual([BEFORE_WINDOW_MS, CALL_WAIT_MS, DETACH_WAIT_MS], [5_000, 11_000, 2_000]);
  for (const tool of EACH_TOOL) {
    const [method] = tool();
    // A browser that answers everything: the call is done when the person has
    // answered, and the console is read for the half second it settles in.
    const whole = await withTool(tool, {});
    assert.equal(whole.seconds, method === "read_console" ? 1 : 0, `${method}: a call whose browser answers waits for nothing`);
    assert.equal(whole.reply.error, undefined, `${method}: ${JSON.stringify(whole.reply)}`);
    // The browser answers the first `step - 1` calls after the person's answer and none after.
    for (let step = 1; step <= whole.calls; step += 1) {
      const label = `${method}, no answer from the call ${step} of ${whole.calls} after the person's answer`;
      const stopped = await withTool(tool, { from: step });
      assert.ok(stopped.seconds !== null, `${label}: no reply`);
      assert.ok(stopped.seconds <= (CALL_WAIT_MS + DETACH_WAIT_MS) / 1000, `${label}: the reply came ${stopped.seconds} seconds after`);
      if (step === whole.calls) {
        // The detach is the last call. The work was done, so the reply is the one a call gets.
        assert.deepEqual(stopped.reply, whole.reply, label);
        assert.equal(stopped.seconds, whole.seconds + DETACH_WAIT_MS / 1000, `${label}: the detach holds the reply for its wait`);
      } else {
        assert.equal(stopped.reply.error?.kind, "failed", label);
      }
      // A console is read after the tab is looked for and attached to, which come after the answer.
      const attaches = method === "read_console" && step <= 2 ? 0 : 1;
      assert.equal(callsOf(stopped.r.fake, "detach").length, attaches, `${label}: detaches`);
      assert.equal(stopped.later, stopped.atReply, `${label}: input was sent after the reply`);
    }
  }
});

test("a call whose browser answers every step after 7 seconds ends 11 seconds after the person's answer with the one sentence for it, though no step is stuck, and sends nothing after the reply", async () => {
  const late = failed("late");
  const ends = new Map([
    ["click_element_trusted", [late, 13]],
    ["type_text_trusted", [late, 13]],
    ["press_key_trusted", [late, 13]],
    ["hover_element", [late, 13]],
    ["drag_element", [late, 13]],
    // The picture has a wait of its own, 10 seconds, which comes first.
    ["screenshot_full_page", [failed("failed"), 12]],
    // The console attaches after the answer, and its attach is not answered by the
    // deadline, so there is nothing to detach for the reply to wait for.
    ["read_console", [late, 11]],
    ["choose_file", [late, 13]],
  ]);
  for (const tool of EACH_TOOL) {
    const [method] = tool();
    const slow = await withTool(tool, { from: 1, delay: 7_000 });
    const [reply, seconds] = ends.get(method);
    assert.deepEqual(slow.reply, reply, method);
    assert.equal(slow.seconds, seconds, `${method}: seconds after the answer`);
    assert.equal(callsOf(slow.r.fake, "detach").length, 1, method);
    assert.equal(slow.later, slow.atReply, `${method}: input was sent after the reply`);
  }
});

test("a console read whose tab list and attach take 9 seconds each ends at the deadline, 11 seconds after the answer, while the attach is waiting, and the attach is detached when the browser answers it", async () => {
  const slow = await withTool(EACH_TOOL[6], { from: 1, delay: 9_000 });
  assert.deepEqual(slow.reply, failed("late"));
  assert.equal(slow.seconds, CALL_WAIT_MS / 1000);
  // The browser answered the attach 2 seconds after the call ended, and the debugger was let go.
  assert.equal(callsOf(slow.r.fake, "detach").length, 1);
  assert.equal(slow.r.fake.attached.size, 0, "the debugger stays attached to the tab");
});

test("a click or a drag that ran out of time sends nothing when the browser answers later: the session ended at the deadline and the work that was left cannot press", async () => {
  for (const tool of [EACH_TOOL[0], EACH_TOOL[4]]) {
    const [method] = tool();
    const whole = await withTool(tool, {});
    // The reads after the first input, which the next press waits for, are answered after 10 seconds each.
    const first = whole.sequence.findIndex((name) => ACTS.test(name));
    const slow = await withTool(tool, { from: first + 2, delay: 10_000 });
    assert.deepEqual(slow.reply, failed("late"), method);
    assert.equal(slow.atReply, 1, `${method}: the move before the read`);
    assert.equal(slow.later, 1, `${method}: input was sent after the reply`);
    assert.equal(slow.seconds, (CALL_WAIT_MS + DETACH_WAIT_MS) / 1000, method);
  }
});

// Whether a button or a key is down according to the input the browser received:
// a press or a key down puts it down, and a release or a key up lifts it.
function heldAfter(fake) {
  const held = { button: false, key: false };
  for (const entry of inputsOf(fake)) {
    if (entry.method === MOUSE && entry.type === "mousePressed") {
      held.button = true;
    } else if (entry.method === MOUSE && entry.type === "mouseReleased") {
      held.button = false;
    } else if (entry.method === KEY && (entry.type === "keyDown" || entry.type === "rawKeyDown")) {
      held.key = true;
    } else if (entry.method === KEY && entry.type === "keyUp") {
      held.key = false;
    }
  }
  return held;
}

// The person answers the window 59 seconds into the 60 they have. From then on the
// browser answers every read, the list of tabs, an attach and a detach `readDelay`
// milliseconds after it is sent, answers each input at once, and receives the press
// (or key down) of the tool and never answers it, nor anything after it. The clock
// runs in steps of a tenth of a second. The answer is the milliseconds the clock
// ran from the person's answer to the reply, the reply, the millisecond the press
// was sent at, how many reads came before it, and the rig. It is the `nth` press
// that the browser receives and does not answer.
async function silentFromThePress(tool, readDelay, nth = 1) {
  const [method, params, world, answers] = tool();
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const state = { answered: false, pressed: false, pressedAt: null, answeredAt: null, reads: 0, presses: 0 };
  const r = rig({
    world,
    answers,
    ask: () =>
      new Promise((resolve) =>
        setTimeout(() => {
          state.answered = true;
          state.answeredAt = Date.now();
          resolve();
        }, CONFIRMATION_WAIT_MS - 1_000),
      ),
  });
  const api = r.fake.chrome.debugger;
  const never = () => new Promise(() => {});
  const isPress = (name, sent) =>
    name === "sendCommand" &&
    ((sent[1] === MOUSE && sent[2]?.type === "mousePressed") ||
      (sent[1] === KEY && (sent[2]?.type === "keyDown" || sent[2]?.type === "rawKeyDown")));
  for (const name of ["getTargets", "attach", "sendCommand", "detach"]) {
    const original = api[name];
    api[name] = (...args) => {
      const settled = original(...args);
      settled.catch(() => {});
      if (!state.answered) {
        return settled;
      }
      if (state.pressed) {
        return never();
      }
      if (isPress(name, [name, ...args.slice(1)]) && (state.presses += 1) === nth) {
        state.pressed = true;
        state.pressedAt = Date.now() - state.answeredAt;
        return never();
      }
      if (name === "sendCommand" && ACTS.test(args[1])) {
        return settled;
      }
      state.reads += 1;
      return readDelay === 0
        ? settled
        : new Promise((resolve, reject) => setTimeout(() => settled.then(resolve, reject), readDelay));
    };
  }
  let reply = null;
  call(r, method, params).then((value) => {
    reply = value;
  });
  await flush();
  mock.timers.tick(CONFIRMATION_WAIT_MS - 1_000);
  await flush();
  let ms = null;
  for (let at = 0; at <= 1_000 && ms === null; at += 1) {
    if (reply !== null) {
      ms = at * 100;
    } else {
      mock.timers.tick(100);
      await flush();
    }
  }
  const atReply = inputsOf(r.fake).length;
  for (let at = 0; at < 300; at += 1) {
    mock.timers.tick(100);
    await flush();
  }
  const later = inputsOf(r.fake).length;
  mock.timers.reset();
  return { ms, reply, r, pressedAt: state.pressedAt, reads: state.reads, atReply, later };
}

// The press-bearing tools, and the last input the browser should have received
// when the call is over: the release of what the tool pressed.
const PRESSING = [
  ["a click", () => ["click_element_trusted", click, fresh(), {}], { method: MOUSE, type: "mouseReleased", button: "left", buttons: 0, clickCount: 1 }],
  ["the second press of a double click", () => ["click_element_trusted", { ...click, count: 2 }, fresh(), {}], { method: MOUSE, type: "mouseReleased", button: "left", buttons: 0, clickCount: 2 }, 2],
  ["a right click", () => ["click_element_trusted", { ...click, button: "right" }, fresh(), {}], { method: MOUSE, type: "mouseReleased", button: "right", buttons: 0, clickCount: 1 }],
  [
    "the select-all key of typing",
    () => {
      const world = fresh();
      return ["type_text_trusted", { role: "textbox", name: "Full name", text: "Ada" }, world, focusing(world, backendOf(world, "textbox", "Full name"))];
    },
    { method: KEY, type: "keyUp", key: "Unidentified", windowsVirtualKeyCode: 0 },
  ],
  [
    "a key press",
    () => {
      const world = fresh();
      focusOn(world, backendOf(world, "textbox", "Full name"));
      return ["press_key_trusted", { key: "Enter" }, world, {}];
    },
    { method: KEY, type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 },
  ],
  ["a drag", () => ["drag_element", { ...click, onto_role: "button", onto_name: "Submit" }, fresh(), {}], { method: MOUSE, type: "mouseReleased", button: "left", buttons: 0, clickCount: 1 }],
];

// How many presses and releases the browser received, and the last input.
function pressesOf(fake) {
  const inputs = inputsOf(fake);
  return {
    presses: inputs.filter((entry) => entry.type === "mousePressed" || entry.type === "keyDown" || entry.type === "rawKeyDown").length,
    releases: inputs.filter((entry) => entry.type === "mouseReleased" || entry.type === "keyUp").length,
    last: inputs.at(-1),
  };
}

test("a call whose time runs out with a button or key pressed releases it before it detaches, once, whichever tool presses: the browser took the press and answers nothing after it", async () => {
  for (const [label, tool, release, nth] of PRESSING) {
    // The reads that come before the press, to put the press where the input's own wait ends after the time.
    const dry = await silentFromThePress(tool, 0, nth);
    const readDelay = Math.floor(8_000 / dry.reads / 100) * 100;
    const silent = await silentFromThePress(tool, readDelay, nth);
    assert.deepEqual(silent.reply, failed("late"), label);
    assert.ok(
      silent.pressedAt > CALL_WAIT_MS - DIALOG_WAIT_MS - INPUT_WAIT_MS && silent.pressedAt < CALL_WAIT_MS,
      `${label}: the press was sent ${silent.pressedAt} ms after the answer, so the call's own wait for it ends after the time`,
    );
    assert.ok(silent.ms <= CALL_WAIT_MS + DETACH_WAIT_MS, `${label}: the reply came ${silent.ms} ms after the answer`);
    const { presses, releases, last } = pressesOf(silent.r.fake);
    for (const [key, value] of Object.entries(release)) {
      assert.deepEqual(last[key], value, `${label}: the last input the browser received was ${JSON.stringify(last)}`);
    }
    assert.deepEqual(heldAfter(silent.r.fake), { button: false, key: false }, `${label}: an input is held`);
    assert.equal(releases, presses, `${label}: ${releases} releases for ${presses} presses`);
    const order = silent.r.fake.calls.map((entry) => (entry.call === "sendCommand" && ACTS.test(entry.method) ? "input" : entry.call));
    assert.ok(order.lastIndexOf("input") < order.indexOf("detach"), `${label}: the release does not come before the detach: ${order.join()}`);
    assert.equal(silent.later, silent.atReply, `${label}: input was sent after the reply`);
    assert.equal(callsOf(silent.r.fake, "detach").length, 1, label);
  }
});

test("a call whose browser takes the press and answers nothing after it, with the reads quick, ends by the call's own release, which is the only one sent, and the end of the call sends no other", async () => {
  for (const [label, tool, release, nth] of PRESSING) {
    const silent = await silentFromThePress(tool, 0, nth);
    assert.deepEqual(silent.reply, failed("failed"), label);
    const { presses, releases, last } = pressesOf(silent.r.fake);
    assert.equal(releases, presses, `${label}: ${releases} releases for ${presses} presses`);
    for (const [key, value] of Object.entries(release)) {
      assert.deepEqual(last[key], value, label);
    }
    assert.deepEqual(heldAfter(silent.r.fake), { button: false, key: false }, label);
  }
});

// The calls below run on a fake clock from the arrival of the request. The browser's
// calls (the list of tabs, an attach, each command, the detach) are counted until
// the person is first asked. From the nth on, the browser does what it is sent
// and answers `delay` milliseconds later, or never when `delay` is Infinity. The
// person answers at once. The answer is the milliseconds the clock ran to the reply,
// the reply, whether the person was asked, how many calls came before they were
// asked in a call the browser answers, and the names of the calls.
async function beforeTheWindow(tool, { from = Infinity, delay = Infinity } = {}) {
  const [method, params, world, answers] = tool();
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const state = { asked: false, calls: 0, sequence: [] };
  const r = rig({
    world,
    answers,
    ask: () => {
      state.asked = true;
    },
  });
  const api = r.fake.chrome.debugger;
  for (const name of ["getTargets", "attach", "sendCommand", "detach"]) {
    const original = api[name];
    api[name] = (...args) => {
      const settled = original(...args);
      settled.catch(() => {});
      if (state.asked) {
        return settled;
      }
      state.calls += 1;
      state.sequence.push(name === "sendCommand" ? args[1] : name);
      if (state.calls < from) {
        return settled;
      }
      return delay === Infinity
        ? new Promise(() => {})
        : new Promise((resolve, reject) => setTimeout(() => settled.then(resolve, reject), delay));
    };
  }
  let reply = null;
  call(r, method, params).then((value) => {
    reply = value;
  });
  await flush();
  let ms = null;
  for (let at = 0; at <= 1_000 && ms === null; at += 1) {
    if (reply !== null) {
      ms = at * 100;
    } else {
      mock.timers.tick(100);
      await flush();
    }
  }
  const atReply = { asked: state.asked, inputs: inputsOf(r.fake).length };
  for (let at = 0; at < 300; at += 1) {
    mock.timers.tick(100);
    await flush();
  }
  mock.timers.reset();
  return { ms, reply, r, ...atReply, calls: state.calls, sequence: state.sequence, askedLater: state.asked };
}

test("the steps before the first window end the call within BEFORE_WINDOW_MS, whichever tool it is and whichever step the browser stops answering at, with the sentence that nothing was done, and the person is not asked", async () => {
  assert.ok(BEFORE_WINDOW_MS + CONFIRMATION_WAIT_MS + CALL_WAIT_MS + DETACH_WAIT_MS <= WAIT_MS - 2_000, "the bounds do not fit in the wait of the first extension");
  for (const tool of EACH_TOOL) {
    const [method] = tool();
    const whole = await beforeTheWindow(tool, {});
    assert.equal(whole.askedLater, true, `${method}: the person is asked when the browser answers`);
    assert.ok(whole.ms < BEFORE_WINDOW_MS, method);
    const attach = whole.sequence.indexOf("attach");
    for (let step = 1; step <= whole.calls; step += 1) {
      const label = `${method}, no answer from the call ${step} of ${whole.calls} before the window`;
      const stopped = await beforeTheWindow(tool, { from: step });
      assert.deepEqual(stopped.reply, failed("slow"), label);
      assert.equal(stopped.askedLater, false, `${label}: the person was asked`);
      assert.equal(stopped.inputs, 0, `${label}: input was sent`);
      // The detach is one of the calls the browser does not answer, if the attach was answered.
      const detaches = attach !== -1 && step > attach + 1 ? 1 : 0;
      assert.equal(stopped.ms, BEFORE_WINDOW_MS + detaches * DETACH_WAIT_MS, `${label}: milliseconds`);
      assert.equal(callsOf(stopped.r.fake, "detach").length, detaches, `${label}: detaches`);
    }
    // A browser that answers every step is not stuck, and each step takes a little more than the
    // time divided by the steps, so the window would come after the time.
    const slow = await beforeTheWindow(tool, { from: 1, delay: Math.floor(BEFORE_WINDOW_MS / whole.calls) + 100 });
    assert.deepEqual(slow.reply, failed("slow"), `${method}: slow`);
    assert.equal(slow.askedLater, false, `${method}: slow, the person was asked`);
    assert.ok(slow.ms <= BEFORE_WINDOW_MS + DETACH_WAIT_MS, `${method}: slow, ${slow.ms} ms`);
  }
});

// Once a token is spent, an approval that answers true to any later token, or to
// one made for another resolution, fails at the loop below.
test("the approval of a call accepts the token the window made for its resolution, as often as it is presented, and no other token, nothing before the person was asked, and nothing that is not a token", async () => {
  const permit = permission();
  try {
    const resolution = Object.freeze({ description: { action: "click" } });
    const other = Object.freeze({ description: { action: "click" } });
    assert.equal(permit.allows(approve(resolution)), false, "a token before the person was asked");
    setConfirmation(async (given) => approve(given));
    const token = await permit.ask(resolution);
    assert.equal(permit.allows(token), true);
    assert.equal(permit.allows(token), true, "the same token a second time, as a click sends several inputs with it");
    for (const [label, presented] of [
      ["another token made for the same resolution", approve(resolution)],
      ["a token made for another resolution", approve(other)],
      ["no token", undefined],
      ["null", null],
      ["an object that is not a token", {}],
      ["a string", "token"],
    ]) {
      assert.equal(permit.allows(presented), false, label);
    }
    assert.equal(permit.allows(token), true, "the token stopped serving after another was refused");
  } finally {
    permit.time.stop();
  }
});

test("no timer of a call is left running when the call ends, whichever tool it is and whether it was done, denied or found no tab", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const live = new Set();
  const set = globalThis.setTimeout;
  const clear = globalThis.clearTimeout;
  globalThis.setTimeout = (...given) => {
    const timer = set(...given);
    live.add(timer);
    return timer;
  };
  globalThis.clearTimeout = (timer) => {
    live.delete(timer);
    return clear(timer);
  };
  try {
    for (const tool of EACH_TOOL) {
      const [method, params, world, answers] = tool();
      for (const [how, options, extra] of [
        ["done", {}, {}],
        ["denied", { allow: false }, {}],
        ["no tab", {}, { url: "https://example.test/nothing" }],
      ]) {
        const r = rig({ world: how === "done" ? world : fresh(), answers: how === "done" ? answers : {}, ...options });
        let reply = null;
        call(r, method, { ...params, ...extra }).then((value) => {
          reply = value;
        });
        for (let at = 0; at < 100 && reply === null; at += 1) {
          await flush();
          mock.timers.tick(100);
        }
        await flush();
        assert.notEqual(reply, null, `${method} ${how}: no reply`);
        assert.equal(reply.error === undefined, how === "done", `${method} ${how}: ${JSON.stringify(reply)}`);
        assert.equal(live.size, 0, `${method}, ${how}: ${live.size} timers left`);
      }
    }
  } finally {
    globalThis.setTimeout = set;
    globalThis.clearTimeout = clear;
  }
});

test("the time before the window stops when the person is asked, so a person who takes the whole minute is not cut short, and the time after the answer is counted from the Allow", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  let allow;
  const r = rig({ ask: () => new Promise((resolve) => (allow = resolve)) });
  let reply = null;
  call(r, "hover_element", click).then((value) => {
    reply = value;
  });
  await flush();
  assert.ok(allow, "the person was not asked");
  mock.timers.tick(CONFIRMATION_WAIT_MS - 1_000);
  await flush();
  assert.equal(reply, null, "the call ended while the person was asked");
  allow();
  await flush();
  assert.deepEqual(reply, done("hovered"));
});

test("a page whose open dialog holds the browser's answers cannot hold the call or its place, and when the dialog is answered later nothing is asked, pressed or detached again: a read before the window, and a read between the move and the press of a click", async () => {
  for (const [stage, held, sentence, ends] of [
    ["before the window", "Page.enable", "slow", BEFORE_WINDOW_MS],
    ["after the person's answer", "Page.getFrameTree", "late", CALL_WAIT_MS],
  ]) {
    mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
    const world = fresh();
    const base = recordedPage(world).answers;
    let answered = 0;
    let dismiss;
    const dialog = new Promise((resolve) => {
      dismiss = resolve;
    });
    const r = rig({
      world,
      answers: {
        // The read after the move of the first call waits for the dialog to be dismissed. The frame tree is read
        // once before the window and once when the press is checked.
        [held]: (params) => {
          answered += 1;
          return (held === "Page.enable" ? answered === 1 : answered === 2) ? dialog.then(() => base[held]?.(params) ?? {}) : (base[held]?.(params) ?? {});
        },
      },
    });
    let first = null;
    call(r, "click_element_trusted", click).then((value) => {
      first = value;
    });
    await flush();
    mock.timers.tick(ends - 1);
    await flush();
    assert.equal(first, null, `${stage}: the call ended early`);
    mock.timers.tick(1);
    await flush();
    mock.timers.tick(DETACH_WAIT_MS);
    await flush();
    assert.deepEqual(first, failed(sentence), `${stage}: the reply`);
    assert.equal(callsOf(r.fake, "detach").length, 1, `${stage}: the debugger is still attached`);
    assert.equal(r.fake.attached.size, 0, stage);
    const windows = r.windows.length;
    const inputs = inputsOf(r.fake).length;
    assert.equal(heldAfter(r.fake).button, false, stage);
    // Its place is free: the next request is answered, and not refused as busy.
    let second = null;
    call(r, "hover_element", click).then((value) => {
      second = value;
    });
    await flush();
    mock.timers.tick(100);
    await flush();
    assert.deepEqual(second, done("hovered"), `${stage}: the next request`);
    const windowsAfter = r.windows.length;
    const inputsAfter = inputsOf(r.fake).length;
    // The person dismisses the page's dialog. What the first call waited for is answered, and nothing follows from it.
    dismiss();
    await flush();
    mock.timers.tick(CONFIRMATION_WAIT_MS);
    await flush();
    assert.equal(r.windows.length, windowsAfter, `${stage}: a window was opened for the call that had ended`);
    assert.equal(inputsOf(r.fake).length, inputsAfter, `${stage}: input was sent for the call that had ended`);
    assert.equal(callsOf(r.fake, "detach").length, 2, `${stage}: detaches`);
    assert.ok(windowsAfter >= windows && inputsAfter >= inputs);
    mock.timers.reset();
  }
});

test("the sentence for a call that ran out of time says the browser did not finish after the person allowed it, and the action may have been done", () => {
  assert.equal(
    SENTENCES.late(),
    "the browser did not finish in time after you allowed it, so the action may or may not have been done",
  );
});

test("each tool answers the status word of what it did, and the eight words are those the table of requests allows", async () => {
  const statuses = new Set();
  const typing = fresh();
  const pressing = fresh();
  focusOn(pressing, backendOf(pressing, "textbox", "Full name"));
  const cases = [
    [rig(), "click_element_trusted", { role: "button", name: "Archive" }, "clicked"],
    [rig(), "click_element_trusted", { role: "button", name: "Archive", count: 2 }, "double clicked"],
    [rig(), "click_element_trusted", { role: "button", name: "Archive", button: "right" }, "right clicked"],
    [
      rig({ world: typing, answers: focusing(typing, backendOf(typing, "textbox", "Full name")) }),
      "type_text_trusted",
      { role: "textbox", name: "Full name", text: "x" },
      "typed",
    ],
    [rig({ world: pressing }), "press_key_trusted", { key: "Tab" }, "pressed"],
    [rig(), "hover_element", { role: "button", name: "Archive" }, "hovered"],
    [rig(), "drag_element", { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" }, "dragged"],
    [rig(), "choose_file", { ...REPORT, name: "Attachment" }, "chose"],
  ];
  for (const [r, method, params, status] of cases) {
    const reply = await call(r, method, params);
    assert.deepEqual(reply, done(status));
    statuses.add(reply.result.status);
  }
  assert.deepEqual([...statuses].sort(), [...STATUSES].sort());
});

// ---- choose_file ----

test("a file is chosen on the one file input a name leads to only after the person allows it, and the browser is asked for that file, on that node, and nothing else", async () => {
  const r = rig();
  const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment" });
  assert.deepEqual(reply, done("chose"));
  const node = backendOf(r.world, "button", "Attachment");
  assert.deepEqual(inputsOf(r.fake), [
    { method: "DOM.setFileInputFiles", files: [REPORT.path], backendNodeId: node },
  ]);
  // The window was asked first, before any input, with the input and the file.
  assert.equal(r.windows.length, 1);
  assert.equal(r.windows[0].inputs, 0);
  const { description } = r.windows[0];
  assert.equal(description.action, "upload");
  assert.equal(description.name, "Attachment");
  assert.equal(description.tag, "input");
  assert.equal(description.origin, ORIGIN);
  assert.equal(description.file, "report.txt, 12 bytes, /srv/relay/uploads/report.txt");
  // Nothing about the page or the file is in the reply but the word and the origin.
  const said = JSON.stringify(reply);
  for (const word of ["Attachment", "report.txt", "/srv"]) {
    assert.equal(said.includes(word), false, word);
  }
});

test("a denial, a window that gives no token and a closed window choose no file", async () => {
  for (const allow of [false, () => null, () => Promise.resolve(undefined)]) {
    const r = rig({ allow });
    const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment" });
    assert.equal(reply.error.kind, "refusal");
    assert.equal(r.windows.length, 1);
    nothingSent(r);
  }
});

test("the role of a file input is button and may be left out, and a role it does not have finds no file input", async () => {
  for (const role of [undefined, "button"]) {
    const r = rig();
    const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment", ...(role ? { role } : {}) });
    assert.deepEqual(reply, done("chose"), String(role));
    assert.equal(r.windows[0].description.role, "button");
    assert.equal(inputsOf(r.fake).length, 1);
  }
  for (const role of ["textbox", "link", "checkbox"]) {
    const r = rig();
    assert.deepEqual(await call(r, "choose_file", { ...REPORT, name: "Attachment", role }), failed("none"), role);
    assert.equal(r.windows.length, 0);
    nothingSent(r);
  }
});

test("the name is compared as the other tools compare it, and a name that is no file input's is not found", async () => {
  const r = rig();
  assert.deepEqual(await call(r, "choose_file", { ...REPORT, name: "  aTTACHMENT  " }), done("chose"));
  for (const name of ["Attach", "Attachment 2", "Choose file"]) {
    const none = rig();
    assert.deepEqual(await call(none, "choose_file", { ...REPORT, name }), failed("none"), name);
    assert.equal(none.windows.length, 0);
    nothingSent(none);
  }
});

test("an element that is not a file input is refused for a file whatever it is called, and the file input refusal of every other tool stays", async () => {
  // The recorded page has a button called "Archive" and a link: neither is a file input.
  for (const [role, name] of [["button", "Archive"], ["link", "Docs"], ["textbox", "Full name"]]) {
    const r = rig();
    const reply = await call(r, "choose_file", { ...REPORT, name, role });
    assert.deepEqual(reply, failed("none"), name);
    assert.equal(r.windows.length, 0, name);
    nothingSent(r);
  }
  // A page that makes a button of its own an <input type=file> after the fact is the
  // same element the other tools refuse: they still do.
  const other = rig();
  assert.deepEqual(
    await call(other, "click_element_trusted", { role: "button", name: "Attachment" }),
    refused("file"),
  );
  nothingSent(other);
});

test("a file input that is disabled or marked as a payment card field is not chosen on, shown or hidden", async () => {
  const card = rig();
  setAttribute(card.world, backendOf(card.world, "button", "Attachment"), "autocomplete", "cc-number");
  assert.deepEqual(await call(card, "choose_file", { ...REPORT, name: "Attachment" }), refused("card"));
  assert.equal(card.windows.length, 0);
  nothingSent(card);

  for (const [name, reply] of [
    ["Locked upload", failed("disabled")],
    ["Frozen upload", failed("disabled")],
    ["Card scan", refused("card")],
  ]) {
    const hidden = rig({ world: hiddenFresh(), url: URL_HIDDEN });
    assert.deepEqual(await call(hidden, "choose_file", { ...REPORT, name }), reply, name);
    assert.equal(hidden.windows.length, 0, name);
    nothingSent(hidden);
  }
});

// A file input the page hides: the window says so, and the browser is given the node.
const hiddenRig = (options) => rig({ world: hiddenFresh(), url: URL_HIDDEN, ...options });

test("a file is chosen on an input the page hides only after the person allows it, in a window that says the page hides it and how the page names it", async () => {
  for (const [name, id, action] of [
    ["Upload resume", "resume", "upload_hidden"],
    ["Cover letter", undefined, "upload_hidden"],
    ["plainname", "upload-field", "upload_hidden_id"],
  ]) {
    const r = hiddenRig();
    const reply = await call(r, "choose_file", { ...REPORT, name });
    assert.deepEqual(reply, done("chose"), name);
    const node =
      id === undefined ? inputWith(r.world, "aria-label", name) : inputWith(r.world, "id", id);
    // The browser gets the node and the one path, and no mouse or key is sent.
    assert.deepEqual(inputsOf(r.fake), [
      { method: "DOM.setFileInputFiles", files: [REPORT.path], backendNodeId: node },
    ], name);
    assert.equal(r.windows.length, 1, name);
    assert.equal(r.windows[0].inputs, 0, name);
    const { description } = r.windows[0];
    assert.equal(description.action, action, name);
    assert.equal(description.name, name);
    assert.equal(description.tag, "input");
    assert.equal(description.role, "button");
    assert.equal(description.file, "report.txt, 12 bytes, /srv/relay/uploads/report.txt");
  }
  // An input that is shown has the words it had.
  const shown = hiddenRig();
  assert.deepEqual(await call(shown, "choose_file", { ...REPORT, name: "Twin" }), done("chose"));
  assert.equal(shown.windows[0].description.action, "upload");
});

test("a denial chooses no file on an input the page hides, and neither does a page that moves, shows or renames it while the person decides", async () => {
  const denied = hiddenRig({ allow: false });
  const reply = await call(denied, "choose_file", { ...REPORT, name: "Upload resume" });
  assert.equal(reply.error.kind, "refusal");
  assert.equal(denied.windows.length, 1);
  nothingSent(denied);

  const moved = hiddenRig({
    ask: (resolution, world) => {
      const row = flatOf(world, inputWith(world, "id", "passport")).parentId;
      moveNode(world, inputWith(world, "id", "resume"), world.flat.find((node) => node.nodeId === row).backendNodeId);
    },
  });
  assert.deepEqual(await call(moved, "choose_file", { ...REPORT, name: "Upload resume" }), failed("changed"));
  nothingSent(moved);

  const renamed = hiddenRig({
    ask: (resolution, world) => setAttribute(world, inputWith(world, "id", "resume"), "aria-label", "Other"),
  });
  assert.deepEqual(await call(renamed, "choose_file", { ...REPORT, name: "Upload resume" }), failed("none"));
  nothingSent(renamed);

  const shown = hiddenRig({
    ask: (resolution, world) => showNode(world, inputWith(world, "class", "clear")),
  });
  assert.deepEqual(await call(shown, "choose_file", { ...REPORT, name: "Attach photo" }), failed("changed"));
  assert.equal(shown.windows[0].description.action, "upload_hidden");
  nothingSent(shown);

  const replaced = hiddenRig({
    ask: (resolution, world) => replaceNode(world, inputWith(world, "id", "resume"), 9_999),
  });
  assert.deepEqual(await call(replaced, "choose_file", { ...REPORT, name: "Upload resume" }), failed("changed"));
  nothingSent(replaced);
});

test("a name no input of the page has is not chosen on and asks nothing, whether the input is hidden or two are", async () => {
  for (const [name, reply] of [
    ["Choose a file", failed("none")],
    ["resume", failed("none")],
    ["Same name", failed("several", 2)],
  ]) {
    const r = hiddenRig();
    assert.deepEqual(await call(r, "choose_file", { ...REPORT, name }), reply, name);
    assert.equal(r.windows.length, 0, name);
    nothingSent(r);
  }
});

test("a file whose path, size or name is not what the relay sends, or that is named like a credential, is refused before the tab is touched", async () => {
  for (const [file, expected] of [
    [{ path: "report.txt" }, refused("argument", "path")],
    [{ path: "/srv/uploads/../secret.txt", file_name: "secret.txt" }, refused("argument", "path")],
    [{ path: "/srv/uploads/a\nb.txt", file_name: "a\nb.txt" }, refused("argument", "path")],
    [{ path: "/srv/uploads/a\u202eb.txt", file_name: "a\u202eb.txt" }, refused("argument", "path")],
    [{ size: -1 }, refused("argument", "size")],
    [{ size: 10 * 1024 * 1024 + 1 }, refused("argument", "size")],
    [{ size: 1.5 }, refused("argument", "size")],
    [{ file_name: "other.txt" }, refused("argument", "file_name")],
    [{ path: "/srv/uploads/.env", file_name: ".env" }, refused("credential")],
    [{ path: "/srv/uploads/ID_RSA", file_name: "ID_RSA" }, refused("credential")],
    [{ path: "/srv/.ssh/notes.txt", file_name: "notes.txt" }, refused("credential")],
    [{ path: "/srv/uploads/server.pem", file_name: "server.pem" }, refused("credential")],
  ]) {
    const r = rig();
    const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment", ...file });
    assert.deepEqual(reply, expected, JSON.stringify(file));
    assert.equal(r.windows.length, 0);
    assert.equal(callsOf(r.fake, "attach").length, 0, JSON.stringify(file));
    assert.equal(callsOf(r.fake, "getTargets").length, 0, JSON.stringify(file));
  }
  // 10 MiB exactly is a file that may be chosen.
  const exact = rig();
  const ten = { ...REPORT, size: 10 * 1024 * 1024, name: "Attachment" };
  assert.deepEqual(await call(exact, "choose_file", ten), done("chose"));
});

test("a file input that the page replaces while the person decides is not chosen on", async () => {
  const r = rig({
    ask: (resolution, world) => replaceNode(world, backendOf(world, "button", "Attachment"), 9_999),
  });
  const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment" });
  assert.deepEqual(reply, failed("changed"));
  assert.equal(r.windows.length, 1);
  nothingSent(r);
});

test("a call that ends any way detaches the debugger once, and a browser that refuses the file ends the call with one sentence", async () => {
  const r = rig({ failOn: { sendCommand: (method) => method === "DOM.setFileInputFiles" && new Error("Could not set input files") } });
  const reply = await call(r, "choose_file", { ...REPORT, name: "Attachment" });
  assert.deepEqual(reply, failed("failed"));
  assert.equal(callsOf(r.fake, "detach").length, 1);
  assert.equal(JSON.stringify(reply).includes("Could not set"), false);
});

test("a reply holds a status word and the origin, or one sentence of the table, and none of the page's words or the request's", async () => {
  const world = fresh();
  const page = ["Archive", "Submit", "Save", "/page.html", "example.test/"];
  for (const [method, params, reply] of [
    ["click_element_trusted", { role: "button", name: "Archive" }, done("clicked")],
    ["click_element_trusted", { role: "button", name: "Save" }, failed("several", 2)],
    ["click_element_trusted", { role: "button", name: "Nothing like it" }, failed("none")],
    ["click_element_trusted", { role: "button", name: "Disabled one" }, failed("disabled")],
    ["type_text_trusted", { role: "textbox", name: "Password", text: "hunter2" }, refused("password")],
  ]) {
    const r = rig({ world: structuredClone(world) });
    const got = await call(r, method, params);
    assert.deepEqual(got, reply);
    const text = JSON.stringify(got);
    for (const word of [...page, params.name, params.text].filter((word) => word !== undefined)) {
      // The origin is the one word of the page that a result holds.
      assert.equal(text.replace(ORIGIN, "").includes(word), false, `${word} is in ${text}`);
    }
  }
});

test("a call that ends any way detaches the debugger from the tab once, and a call whose input the browser refuses or does not answer ends with one sentence", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const stuck = rig({ answers: { [MOUSE]: () => new Promise(() => {}) } });
  const waiting = call(stuck, "hover_element", { role: "button", name: "Archive" });
  let reply = null;
  waiting.then((value) => {
    reply = value;
  });
  await flush();
  mock.timers.tick(DIALOG_WAIT_MS);
  await flush();
  assert.equal(reply, null, "the call waits for the browser a while longer");
  mock.timers.tick(INPUT_WAIT_MS - 1);
  await flush();
  assert.equal(reply, null);
  mock.timers.tick(1);
  await flush();
  assert.deepEqual(reply, failed("failed"));
  assert.equal(callsOf(stuck.fake, "detach").length, 1);
  assert.equal(stuck.fake.attached.size, 0);
  mock.timers.reset();

  const cases = {
    "the browser refuses an input": [
      () => rig({ failOn: { sendCommand: (method) => (method === MOUSE ? new Error("boom: SECRET") : undefined) } }),
      failed("failed"),
    ],
    "the person denies": [() => rig({ allow: false }), refused("denied")],
    "the hit test fails": [
      () => rig({ ask: (resolution, world) => coverAt(world, middle("button", "Archive", world)) }),
      failed("covered"),
    ],
  };
  for (const [label, [make, reply]] of Object.entries(cases)) {
    const r = make();
    assert.deepEqual(await call(r, "click_element_trusted", { role: "button", name: "Archive" }), reply, label);
    assert.equal(callsOf(r.fake, "attach").length, 1, label);
    assert.equal(callsOf(r.fake, "detach").length, 1, label);
    assert.equal(r.fake.attached.size, 0, label);
  }
});

test("a request with an argument the tool does not take, a coordinate, or a count out of range is refused before the browser is asked for anything", async () => {
  const good = {
    click_element_trusted: { role: "button", name: "Archive" },
    type_text_trusted: { role: "textbox", name: "Full name", text: "x" },
    press_key_trusted: { key: "Enter" },
    hover_element: { role: "button", name: "Archive" },
    drag_element: { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" },
  };
  for (const [method, params] of Object.entries(good)) {
    for (const extra of [
      { x: 10, y: 20 },
      { clientX: 1 },
      { selector: "#save" },
      { nodeId: 3 },
      { backendNodeId: 82 },
      { tabId: 7 },
      { modifiers: 4 },
      { coordinates: [1, 2] },
    ]) {
      const r = rig();
      assert.deepEqual(await call(r, method, { ...params, ...extra }), refused("extra"), `${method} ${Object.keys(extra)}`);
      assert.deepEqual(r.fake.calls, [], `${method} ${Object.keys(extra)}`);
      assert.equal(r.windows.length, 0);
    }
  }
  for (const count of [0, 3, -1, 1.5, "2", null, [2]]) {
    const r = rig();
    assert.deepEqual(
      await call(r, "click_element_trusted", { role: "button", name: "Archive", count }),
      refused("argument", "count"),
      JSON.stringify(count),
    );
    assert.deepEqual(r.fake.calls, []);
  }
  const right = rig();
  assert.deepEqual(
    await call(right, "click_element_trusted", { role: "button", name: "Archive", button: "right", count: 2 }),
    refused("argument", "count"),
  );
  assert.deepEqual(right.fake.calls, []);
  for (const [params, name] of [
    [{ key: "Control+A" }, "key"],
    [{ key: "a" }, "key"],
    [{ key: "Shift" }, "key"],
    [{ key: "enter" }, "key"],
  ]) {
    const r = rig();
    assert.deepEqual(await call(r, "press_key_trusted", params), refused("argument", name), params.key);
    assert.deepEqual(r.fake.calls, []);
  }
});

// The real windows over fake pages, for the tests of a dialog a click opens.
function dialogRig({ on, dialog, onAnswer, method = MOUSE, prepare }) {
  const panels = browser();
  const world = fresh();
  prepare?.(world);
  const page = recordedPage(world);
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const answered = [];
  const fake = fakeDebugger({
    targets: [pageAt(URL_A, 7)],
    answers: {
      ...page.answers,
      [method]: (params) => {
        if (params.type !== on) {
          return {};
        }
        Promise.resolve().then(() =>
          fake.emit("Page.javascriptDialogOpening", { ...dialog, url: URL_A, hasBrowserHandler: false }, 7),
        );
        return held;
      },
      "Page.handleJavaScriptDialog": (params) => {
        answered.push(params);
        onAnswer?.(world);
        release({});
        return {};
      },
    },
  });
  const chrome = { ...panels.chrome, debugger: fake.chrome.debugger };
  const windows = createWindows(chrome);
  windows.listen();
  setConfirmation(windows.ask);
  setDialogPanel(windows.askDialog);
  return { panels, fake, chrome, answered, world };
}

const followed = (promise) => {
  const state = { done: false };
  state.promise = promise.then((value) => Object.assign(state, { done: true, value }));
  return state;
};

const tick = async (ms) => {
  mock.timers.tick(ms);
  await flush();
};

const lastPage = (panels, file) => panels.pages.findLast((page) => page.file === file);

async function allowInWindow(panels) {
  await flush();
  const window = lastPage(panels, "confirm.html");
  await tick(ALLOW_DELAY_MS);
  await window.click("allow");
  await flush();
}

test("a click that opens a dialog, on the release or on the press, goes to the panel: the person answers it, the page is given the answer once, and the call goes on", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  for (const [on, expected] of [
    ["mouseReleased", ["mouseMoved", "mousePressed", "mouseReleased", "answer"]],
    ["mousePressed", ["mouseMoved", "mousePressed", "answer", "mouseReleased"]],
  ]) {
    const r = dialogRig({ on, dialog: { type: "confirm", message: "Delete everything?", defaultPrompt: "" } });
    const call_ = followed(answer(request("click_element_trusted", { role: "button", name: "Archive" }), r.chrome));
    await allowInWindow(r.panels);
    const panel = lastPage(r.panels, "dialog.html");
    assert.ok(panel, `${on}: no panel opened`);
    assert.equal(call_.done, false);
    assert.deepEqual(r.answered, [], `${on}: the dialog was answered before the person chose`);
    await tick(ALLOW_DELAY_MS);
    await panel.click("accept");
    await call_.promise;
    assert.deepEqual(call_.value, done("clicked"), on);
    assert.deepEqual(r.answered, [{ accept: true }], on);
    assert.deepEqual(
      inputsOf(r.fake).map((entry) => (entry.method === "Page.handleJavaScriptDialog" ? "answer" : entry.type)),
      expected,
      on,
    );
    assert.equal(callsOf(r.fake, "detach").length, 1);
    assert.deepEqual(r.panels.windowsOpen(), []);
  }
});

test("a dialog nobody answers is left open: the call fails with the one sentence, the release is not sent, nothing answers the dialog for the person, and the debugger is detached", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const r = dialogRig({ on: "mousePressed", dialog: { type: "alert", message: "Saved", defaultPrompt: "" } });
  const call_ = followed(answer(request("click_element_trusted", { role: "button", name: "Archive" }), r.chrome));
  await allowInWindow(r.panels);
  const panel = lastPage(r.panels, "dialog.html");
  r.panels.close(panel.opened.id);
  await call_.promise;
  assert.deepEqual(call_.value, failed("dialog_open"));
  assert.deepEqual(r.answered, []);
  assert.deepEqual(mouseOf(r.fake).map((entry) => entry.split(" ")[0]), ["mouseMoved", "mousePressed"]);
  assert.equal(callsOf(r.fake, "detach").length, 1);
});

test("a key whose down or up opens a dialog goes to the panel: once the person has answered it the key is up once and the call goes on; a dialog nobody answers sends no key up, since the browser answers no input", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const dialog = { type: "confirm", message: "Archive it?", defaultPrompt: "" };
  const prepare = (world) => focusOn(world, backendOf(world, "button", "Archive"));
  const events = (fake) =>
    inputsOf(fake).map((entry) => (entry.method === "Page.handleJavaScriptDialog" ? "answer" : entry.type));
  for (const [on, expected] of [
    ["keyDown", ["keyDown", "answer", "keyUp"]],
    ["keyUp", ["keyDown", "keyUp", "answer"]],
  ]) {
    const r = dialogRig({ on, dialog, method: KEY, prepare });
    const call_ = followed(answer(request("press_key_trusted", { key: "Enter" }), r.chrome));
    await allowInWindow(r.panels);
    const panel = lastPage(r.panels, "dialog.html");
    assert.ok(panel, `${on}: no panel opened`);
    assert.deepEqual(r.answered, [], `${on}: the dialog was answered before the person chose`);
    await tick(ALLOW_DELAY_MS);
    await panel.click("accept");
    await call_.promise;
    assert.deepEqual(call_.value, done("pressed"), on);
    assert.deepEqual(events(r.fake), expected, on);
    assert.equal(callsOf(r.fake, "detach").length, 1);

    const open = dialogRig({ on, dialog, method: KEY, prepare });
    const abandoned = followed(answer(request("press_key_trusted", { key: "Enter" }), open.chrome));
    await allowInWindow(open.panels);
    open.panels.close(lastPage(open.panels, "dialog.html").opened.id);
    await abandoned.promise;
    assert.deepEqual(abandoned.value, failed("dialog_open"), on);
    assert.deepEqual(open.answered, [], on);
    assert.deepEqual(events(open.fake), expected.slice(0, on === "keyDown" ? 1 : 2), on);
    assert.equal(callsOf(open.fake, "detach").length, 1);
  }
});

test("a drag whose press opens a dialog: once the person has answered it, a cover over the second element ends the drag with the button released where it was pressed; a dialog nobody answers sends no release, since the browser answers no input", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const drag = { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" };
  const from = middle("button", "Archive");
  const to = middle("button", "Submit");
  const dialog = { type: "confirm", message: "Move it?", defaultPrompt: "" };

  const covered = dialogRig({ on: "mousePressed", dialog, onAnswer: (world) => coverAt(world, to) });
  const finished = followed(answer(request("drag_element", drag), covered.chrome));
  await allowInWindow(covered.panels);
  const panel = lastPage(covered.panels, "dialog.html");
  await tick(ALLOW_DELAY_MS);
  await panel.click("accept");
  await finished.promise;
  assert.deepEqual(finished.value, failed("covered"));
  assert.deepEqual(covered.answered, [{ accept: true }]);
  const events = mouseOf(covered.fake);
  assert.equal(events.length, 11, events.join("; "));
  assert.deepEqual(events.slice(0, 2), [`mouseMoved ${from.x},${from.y}`, `mousePressed ${from.x},${from.y}`]);
  assert.equal(events.at(-1), `mouseReleased ${from.x},${from.y}`);
  assert.equal(callsOf(covered.fake, "detach").length, 1);

  const open = dialogRig({ on: "mousePressed", dialog });
  const abandoned = followed(answer(request("drag_element", drag), open.chrome));
  await allowInWindow(open.panels);
  open.panels.close(lastPage(open.panels, "dialog.html").opened.id);
  await abandoned.promise;
  assert.deepEqual(abandoned.value, failed("dialog_open"));
  assert.deepEqual(open.answered, []);
  assert.deepEqual(mouseOf(open.fake).map((entry) => entry.split(" ")[0]), ["mouseMoved", "mousePressed"]);
  assert.equal(callsOf(open.fake, "detach").length, 1);
});

test("the time of a call does not run while the person answers a dialog in the panel, and runs again from their answer: a drag whose press opens a dialog is not ended by the deadline during 40 seconds in the panel, and is 11 seconds after the answer if the browser is slow", async () => {
  mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] });
  const drag = { role: "button", name: "Archive", onto_role: "button", onto_name: "Submit" };
  const r = dialogRig({ on: "mousePressed", dialog: { type: "confirm", message: "Move it?", defaultPrompt: "" } });
  // After the person's answer, the browser answers a read 9 seconds after it is sent.
  let slow = false;
  const api = r.fake.chrome.debugger;
  const original = api.sendCommand;
  api.sendCommand = (target, method, params) => {
    const settled = original(target, method, params);
    settled.catch(() => {});
    return slow && !ACTS.test(method)
      ? new Promise((resolve, reject) => setTimeout(() => settled.then(resolve, reject), 9_000))
      : settled;
  };
  const finished = followed(answer(request("drag_element", drag), r.chrome));
  await allowInWindow(r.panels);
  const panel = lastPage(r.panels, "dialog.html");
  assert.ok(panel, "no panel opened");
  for (let second = 0; second < 40; second += 1) {
    await tick(1_000);
  }
  assert.equal(finished.done, false, "the call ended while the person read the dialog");
  slow = true;
  await panel.click("accept");
  let seconds = 0;
  while (!finished.done && seconds < 60) {
    await tick(1_000);
    seconds += 1;
  }
  assert.deepEqual(finished.value, failed("late"));
  assert.equal(seconds, CALL_WAIT_MS / 1000, "seconds from the person's answer to the dialog");
  assert.equal(callsOf(r.fake, "detach").length, 1);
});

test("every code a script throws is a sentence of the table, so that no failure is sent as the generic one by a misspelled code", () => {
  const thrown = new Set();
  for (const { name, code } of scripts()) {
    for (const [, kind, found] of code.join("\n").matchAll(/new (Failure|Refusal)\(\s*"([a-z_]+)"/g)) {
      thrown.add(found);
      assert.ok(Object.hasOwn(SENTENCES, found), `${name} throws ${kind}("${found}")`);
    }
  }
  // The scan reads the scripts that throw, and the codes of the tools are among them.
  for (const found of ["denied", "changed", "nofocus", "unsupported", "covered", "offscreen", "left", "password", "dialog_open"]) {
    assert.ok(thrown.has(found), `${found} is thrown nowhere`);
  }
});

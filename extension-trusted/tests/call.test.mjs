// A call of the kind the five tools make, put together from the parts of this
// extension against a fake debugger and the fake windows: it asks the person in
// the confirmation window, sends a click, and when the click opens a dialog asks
// the person in the dialog panel and answers the page with what they chose. The
// tool itself is not here; the call below stands in for it.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import { ALLOW_DELAY_MS, CONFIRMATION_WAIT_MS, confirm, setConfirmation, spend } from "../approval.js";
import { describe } from "../actions.js";
import { command, handleDialog, sendWatching, withSession } from "../cdp.js";
import { Failure, Refusal } from "../sentences.js";
import { createWindows, setDialogPanel, askDialog } from "../windows.js";
import { callsOf, fakeDebugger, pageAt } from "./fake-debugger.mjs";
import { browser, flush } from "./fake-panels.mjs";

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] }));
afterEach(() => mock.timers.reset());

const tick = async (ms) => {
  mock.timers.tick(ms);
  await flush();
};

const URL_A = "https://shop.example/cart";
const ELEMENT = { tag: "button", role: "button", name: "Remove", excerpt: "Remove", goes: "" };
const UP = { type: "mouseReleased", x: 5, y: 6, button: "left", clickCount: 1 };

// A page that opens `dialog` (or none) when the click is released, and keeps the
// release unanswered until the dialog is answered, as a browser does.
function world({ dialog = null } = {}) {
  const panels = browser();
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const answered = [];
  const debuggerFake = fakeDebugger({
    targets: [pageAt(URL_A, 7)],
    answers: {
      "Input.dispatchMouseEvent": (params) => {
        if (params.type !== "mouseReleased" || dialog === null) {
          return {};
        }
        Promise.resolve().then(() =>
          debuggerFake.emit(
            "Page.javascriptDialogOpening",
            { ...dialog, url: URL_A, hasBrowserHandler: false },
            7,
          ),
        );
        return held;
      },
      "Page.handleJavaScriptDialog": (params) => {
        answered.push(params);
        release({});
        return {};
      },
    },
  });
  const chrome = { ...panels.chrome, debugger: debuggerFake.chrome.debugger };
  const windows = createWindows(chrome);
  windows.listen();
  setConfirmation(windows.ask);
  setDialogPanel(windows.askDialog);
  return { panels, debuggerFake, chrome, answered };
}

// What a tool of this extension does for a click, as far as the window and the
// dialog go: ask, press, release, and answer a dialog the release opened. It
// returns the steps it took.
async function click(chrome) {
  const steps = [];
  const holder = { resolution: null, ok: false };
  await withSession(
    chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      const description = describe("click", {
        origin: "https://shop.example",
        url: URL_A,
        element: ELEMENT,
      });
      const resolution = Object.freeze({ description });
      holder.resolution = resolution;
      const token = await confirm(resolution);
      steps.push("allowed");
      await command(session, "Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 6 }, token);
      await command(session, "Input.dispatchMouseEvent", { ...UP, type: "mousePressed" }, token);
      const sent = await sendWatching(session, "Input.dispatchMouseEvent", UP, token);
      steps.push(sent.dialog === null ? "no dialog" : `dialog ${sent.dialog.type}`);
      if (sent.dialog !== null) {
        const answer = await askDialog(sent.dialog, {
          origin: "https://shop.example",
          url: URL_A,
          first: resolution,
        });
        steps.push(answer.accept ? "accepted" : "dismissed");
        await handleDialog(session, answer);
      }
      await sent.done;
      steps.push("done");
    },
    { allows: (token) => (holder.ok ||= spend(token, holder.resolution)) },
  );
  return steps;
}

// The person allowing the action in the window that opened for it.
async function allowAction(panels) {
  await flush();
  const window = panels.pages.findLast((page) => page.file === "confirm.html");
  await tick(ALLOW_DELAY_MS);
  await window.click("allow");
}

const followed = (promise) => {
  const state = { done: false };
  state.promise = promise.then(
    (value) => Object.assign(state, { done: true, value }),
    (error) => Object.assign(state, { done: true, error }),
  );
  return state;
};

const sentMethods = (fake) => callsOf(fake, "sendCommand").map((call) => call.method);

test("a click that opens a dialog is answered once, by the person, in the panel: Accept answers the page true, and the call ends with the click done", async () => {
  const { panels, debuggerFake, chrome, answered } = world({ dialog: { type: "confirm", message: "Delete everything?", defaultPrompt: "" } });
  const call = followed(click(chrome));
  await allowAction(panels);
  await flush();
  const panel = panels.pages.findLast((page) => page.file === "dialog.html");
  assert.ok(panel, "no panel opened");
  assert.equal(call.done, false);
  assert.deepEqual(answered, [], "the dialog was answered before the person chose");
  await tick(ALLOW_DELAY_MS);
  await panel.click("accept");
  await call.promise;
  assert.equal(call.error, undefined);
  assert.deepEqual(call.value, ["allowed", "dialog confirm", "accepted", "done"]);
  assert.deepEqual(answered, [{ accept: true }], "the dialog was not answered once");
  assert.deepEqual(sentMethods(debuggerFake).slice(0, 5), [
    "Page.enable",
    "Input.dispatchMouseEvent",
    "Input.dispatchMouseEvent",
    "Input.dispatchMouseEvent",
    "Page.handleJavaScriptDialog",
  ]);
  assert.equal(callsOf(debuggerFake, "detach").length, 1);
  assert.deepEqual(panels.windowsOpen(), []);
});

test("a dialog the person dismisses is answered false, and the text they typed for a prompt is what the page is given", async () => {
  const cases = [
    [{ type: "confirm", message: "m", defaultPrompt: "" }, (panel) => panel.click("dismiss"), { accept: false }],
    [
      { type: "prompt", message: "Name?", defaultPrompt: "Ada" },
      async (panel) => {
        panel.type("answer", "Grace");
        await panel.click("accept");
      },
      { accept: true, promptText: "Grace" },
    ],
    [{ type: "prompt", message: "Name?", defaultPrompt: "Ada" }, (panel) => panel.key("Escape"), { accept: false }],
  ];
  for (const [dialog, act, expected] of cases) {
    const { panels, chrome, answered } = world({ dialog });
    const call = followed(click(chrome));
    await allowAction(panels);
    await flush();
    const panel = panels.pages.findLast((page) => page.file === "dialog.html");
    await tick(ALLOW_DELAY_MS);
    await act(panel);
    await call.promise;
    assert.equal(call.error, undefined, JSON.stringify(expected));
    assert.deepEqual(answered, [expected]);
  }
});

test("a click that opens no dialog asks nothing more, opens no panel and answers no dialog", async () => {
  const { panels, debuggerFake, chrome, answered } = world();
  const call = followed(click(chrome));
  await allowAction(panels);
  await call.promise;
  assert.deepEqual(call.value, ["allowed", "no dialog", "done"]);
  assert.equal(panels.pages.some((page) => page.file === "dialog.html"), false);
  assert.deepEqual(answered, []);
  assert.equal(sentMethods(debuggerFake).includes("Page.handleJavaScriptDialog"), false);
});

test("a dialog nobody answers is left open and fails the call with one sentence: nothing is answered, and the session is detached", async () => {
  const ways = {
    "the panel is closed": (panels, panel) => panels.close(panel.opened.id),
    "no answer comes in time": () => tick(CONFIRMATION_WAIT_MS),
  };
  for (const [way, end] of Object.entries(ways)) {
    const { panels, debuggerFake, chrome, answered } = world({ dialog: { type: "alert", message: "Saved", defaultPrompt: "" } });
    const call = followed(click(chrome));
    await allowAction(panels);
    await flush();
    const panel = panels.pages.findLast((page) => page.file === "dialog.html");
    await end(panels, panel);
    await call.promise;
    assert.ok(call.error instanceof Failure && call.error.code === "dialog_open", way);
    assert.deepEqual(answered, [], `${way}: the dialog was answered for the person`);
    assert.equal(sentMethods(debuggerFake).includes("Page.handleJavaScriptDialog"), false, way);
    assert.equal(callsOf(debuggerFake, "detach").length, 1, way);
    assert.deepEqual(panels.windowsOpen(), [], way);
  }
});

test("the call has one deadline, counted from the first window: the time the person took to allow the action is not given again to the panel", async () => {
  const { panels, chrome, answered } = world({ dialog: { type: "confirm", message: "m", defaultPrompt: "" } });
  const call = followed(click(chrome));
  await flush();
  await tick(CONFIRMATION_WAIT_MS - 5_000);
  const window = panels.pages.findLast((page) => page.file === "confirm.html");
  await window.click("allow");
  await flush();
  const panel = panels.pages.findLast((page) => page.file === "dialog.html");
  assert.match(panel.element("wait").textContent, /in 5 seconds\.$/);
  await tick(4_999);
  assert.equal(call.done, false);
  await tick(1);
  await call.promise;
  assert.equal(call.error.code, "dialog_open");
  assert.deepEqual(answered, []);
});

test("an action the person denies sends no input, watches for no dialog and opens no panel", async () => {
  const { panels, debuggerFake, chrome } = world({ dialog: { type: "confirm", message: "m", defaultPrompt: "" } });
  const call = followed(click(chrome));
  await flush();
  await panels.pages.findLast((page) => page.file === "confirm.html").click("deny");
  await call.promise;
  assert.ok(call.error instanceof Refusal && call.error.code === "denied");
  assert.deepEqual(sentMethods(debuggerFake), ["Page.enable"]);
  assert.equal(panels.pages.some((page) => page.file === "dialog.html"), false);
});

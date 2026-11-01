// The service worker's side of the two windows (windows.js, over the shared
// confirmation.js), against the fake browser and the real pages of fake-panels.mjs.
// The tests act as the person, pressing the buttons of the pages, and as a page
// or a stale window trying to answer a question that is not its own.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import { ALLOW_DELAY_MS, CONFIRMATION_WAIT_MS, spend } from "../approval.js";
import { Failure, Refusal, SENTENCES, errorOf } from "../sentences.js";
import { createWindows, reasonOf } from "../windows.js";
import { ToolError, Refusal as WindowRefusal } from "../errors.js";
import { EXTENSION, addressOf, browser, flush } from "./fake-panels.mjs";

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] }));
afterEach(() => mock.timers.reset());

const tick = async (ms) => {
  mock.timers.tick(ms);
  await flush();
};

const ACTION = Object.freeze({
  action: "click",
  origin: "https://shop.example",
  url: "https://shop.example/cart",
  tag: "button",
  role: "button",
  name: "Pay now",
  excerpt: "Pay now",
  goes: "",
});
const DIALOG = Object.freeze({ type: "prompt", message: "Your name?", defaultPrompt: "Ada" });
const WHERE = { origin: "https://shop.example", url: "https://shop.example/cart" };

// A fake browser with the windows listening, and the follow-up of a question
// without waiting for it.
function started(options) {
  const fake = browser(options);
  const windows = createWindows(fake.chrome);
  windows.listen();
  return { fake, windows };
}

function following(promise) {
  const state = { done: false, value: undefined, error: undefined };
  state.promise = promise.then(
    (value) => {
      state.done = true;
      state.value = value;
    },
    (error) => {
      state.done = true;
      state.error = error;
    },
  );
  return state;
}

// The action asked about, the window open and the person ready to answer.
async function asked(windows, fake, description = ACTION) {
  const resolution = Object.freeze({ description });
  const question = following(windows.ask(resolution));
  question.resolution = resolution;
  await flush();
  return question;
}

// The dialog panel asked about, its page open.
async function askedDialog(windows, first, dialog = DIALOG) {
  const question = following(windows.askDialog(dialog, { ...WHERE, first }));
  await flush();
  return question;
}

const plain = (page, id) => page.element(id).textContent;
const created = (fake) => fake.calls.filter(([call]) => call === "windows.create");

test("an action the person does not allow ends in a refusal of this extension's table, one for each way, and none holds a word of the window", async () => {
  const ways = {
    denied: [(fake, page) => page.click("deny")],
    "denied by Escape": [(fake, page) => page.key("Escape"), "denied"],
    "denied by an answer that is not allow": [
      (fake, page) => page.ports.page.postMessage({ type: "answer", allow: "yes" }),
      "denied",
    ],
    closed: [(fake, page) => fake.close(page.opened.id)],
    "closed by the port": [(fake, page) => page.ports.page.disconnect(), "closed"],
    timed_out: [() => tick(CONFIRMATION_WAIT_MS)],
  };
  for (const [way, [act, code = way]] of Object.entries(ways)) {
    const { fake, windows } = started();
    const question = await asked(windows, fake);
    await tick(ALLOW_DELAY_MS);
    await act(fake, fake.page);
    await flush();
    assert.equal(question.done, true, `${way} did not end the question`);
    await question.promise;
    assert.ok(question.error instanceof Refusal, way);
    assert.equal(question.error.code, code, way);
    const reply = errorOf(question.error);
    assert.deepEqual(reply, { kind: "refusal", message: SENTENCES[code]() }, way);
    assert.doesNotMatch(reply.message, /browser|denied the action|answer was given/, way);
    assert.deepEqual(fake.windowsOpen(), [], `${way} left the window open`);
  }
});

test("an action the person allows gives the token of the shared window, for the resolution asked about", async () => {
  const { fake, windows } = started();
  const question = await asked(windows, fake);
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await question.promise;
  assert.equal(question.error, undefined);
  assert.equal(spend(question.value, question.resolution), true);
  assert.equal(spend(question.value, question.resolution), false, "a token served twice");
  assert.deepEqual(fake.windowsOpen(), []);
});

test("a window that cannot be opened is a failure of the table, another action waiting is a refusal, and an error of no known kind is the generic failure", async () => {
  const { fake, windows } = started({ failing: new Error("No current window") });
  const lost = await asked(windows, fake);
  assert.ok(lost.error instanceof Failure && !(lost.error instanceof Refusal));
  assert.equal(lost.error.code, "no_window");
  assert.doesNotMatch(errorOf(lost.error).message, /No current window/);

  const second = started();
  const first = await asked(second.windows, second.fake);
  const busy = await asked(second.windows, second.fake);
  assert.ok(busy.error instanceof Refusal);
  assert.equal(busy.error.code, "waiting");
  assert.equal(created(second.fake).length, 1);
  assert.equal(first.done, false);

  const odd = reasonOf(new WindowRefusal("a sentence the shared window has not said before"));
  assert.ok(odd instanceof Failure);
  assert.equal(odd.code, "failed");
  assert.equal(reasonOf(new ToolError("could not be opened")).code, "no_window");
  const other = new TypeError("x");
  assert.equal(reasonOf(other), other);
  assert.deepEqual(errorOf(reasonOf(other)), errorOf(other));
});

test("a dialog is asked about in a panel that shows it, and the person's answer and the text they typed come back with a token for that dialog", async () => {
  const cases = [
    ["Accept with a typed answer", (page) => { page.type("answer", "Grace"); return page.click("accept"); }, { accept: true, text: "Grace" }],
    ["Dismiss", (page) => { page.type("answer", "Grace"); return page.click("dismiss"); }, { accept: false, text: "" }],
    ["Escape", (page) => page.key("Escape"), { accept: false, text: "" }],
  ];
  for (const [way, act, expected] of cases) {
    const { fake, windows } = started();
    const action = await asked(windows, fake);
    await tick(ALLOW_DELAY_MS);
    await fake.page.click("allow");
    await action.promise;
    await tick(1000);
    const question = await askedDialog(windows, action.resolution);
    const panel = fake.page;
    assert.equal(panel.file, "dialog.html", way);
    assert.deepEqual(created(fake).map(([, options]) => options.url), [
      addressOf("confirm.html"),
      addressOf("dialog.html"),
    ]);
    // Both are wide enough for the browser's debugging notice to be read in full
    // (confirmation.test.mjs says how wide that is).
    assert.deepEqual(created(fake).map(([, options]) => options.width), [600, 600], way);
    await tick(ALLOW_DELAY_MS);
    await act(panel);
    await question.promise;
    assert.equal(question.error, undefined, way);
    const answer = question.value;
    assert.equal(answer.accept, expected.accept, way);
    assert.equal(answer.text, expected.text, way);
    assert.deepEqual(answer.resolution.dialog, DIALOG);
    assert.deepEqual(answer.resolution.description, { ...DIALOG, ...WHERE });
    assert.equal(spend(answer.token, answer.resolution), true, way);
    assert.deepEqual(fake.windowsOpen(), [], `${way} left the panel open`);
  }
});

test("the panel and the action window are two questions that cannot answer each other, and neither port is sent away by the other's confirmation", async () => {
  const { fake, windows } = started();
  const action = await asked(windows, fake);
  const actionPage = fake.page;
  // A dialog is asked about while the action window is still open.
  const dialog = await askedDialog(windows, action.resolution);
  const panel = fake.page;
  assert.equal(actionPage.file, "confirm.html");
  assert.equal(panel.file, "dialog.html");
  assert.equal(actionPage.ports.worker.sent.at(-1).type, "show", "the action window was sent away");
  assert.equal(panel.ports.worker.sent[0].type, "show", "the panel was sent away");
  await tick(ALLOW_DELAY_MS);
  await panel.click("accept");
  await dialog.promise;
  assert.equal(dialog.error, undefined);
  assert.equal(action.done, false, "the panel answered the action");
  await actionPage.click("allow");
  await action.promise;
  assert.equal(spend(action.value, action.resolution), true);
  assert.equal(spend(action.value, dialog.value.resolution), false, "an action token served the dialog");
  assert.equal(spend(dialog.value.token, action.resolution), false, "a dialog token served the action");
});

test("a page with no question waiting for it is sent away and its answer does nothing, in either direction", async () => {
  const { fake, windows } = started({ opening: false });
  const action = await asked(windows, fake);
  const answer = { type: "answer", allow: true, accept: true, text: "x" };
  // A dialog panel while only an action is waiting.
  const stray = await fake.open({ file: "dialog.html", window: { id: 70, tabs: [{ id: 700 }] }, early: [answer] });
  assert.deepEqual(stray.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(stray.closed, true);
  assert.equal(action.done, false, "a panel with no question answered the action");
  // The same action window opened for the question is the only one that answers it.
  const created0 = fake.created[0];
  const real = await fake.open({ window: created0 });
  await tick(ALLOW_DELAY_MS);
  await real.click("deny");
  await action.promise;
  assert.equal(action.error.code, "denied");

  // An action window while only a dialog is waiting.
  const second = started({ opening: false });
  const first = await asked(second.windows, second.fake);
  const dialog = await askedDialog(second.windows, first.resolution);
  const wrong = await second.fake.open({ file: "confirm.html", window: { id: 71, tabs: [{ id: 710 }] }, early: [{ type: "answer", allow: true }] });
  assert.deepEqual(wrong.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(dialog.done, false, "an action window answered the dialog");
  assert.equal(first.done, false);
});

test("a page is handed to the confirmation made for its own page, whichever page opens first", async () => {
  const { fake, windows } = started({ opening: false });
  const action = await asked(windows, fake);
  const dialog = await askedDialog(windows, action.resolution);
  const [actionWindow, panelWindow] = fake.created;
  // The panel's question is the later of the two, and its page opens first.
  const panel = await fake.open({ window: panelWindow });
  assert.equal(panel.file, "dialog.html");
  assert.equal(panel.ports.worker.sent[0].type, "show", "the panel was not handed to its confirmation");
  const confirmation = await fake.open({ window: actionWindow });
  assert.equal(confirmation.file, "confirm.html");
  assert.equal(confirmation.ports.worker.sent[0].type, "show", "the window was not handed to its confirmation");
  await tick(ALLOW_DELAY_MS);
  await panel.click("dismiss");
  await dialog.promise;
  assert.equal(dialog.value.accept, false);
  assert.equal(action.done, false);
  await confirmation.click("deny");
  await action.promise;
  assert.equal(action.error.code, "denied");
});

test("the answer to a dialog is taken from the window the shared confirmation admitted, and not from a second page, a page of another extension or a page that sent it before it was shown", async () => {
  const { fake, windows } = started({ opening: false });
  const action = await asked(windows, fake);
  await fake.open({ window: fake.created[0] });
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await action.promise;
  const dialog = await askedDialog(windows, action.resolution);
  const made = fake.created.at(-1);
  const real = await fake.open({ window: made });
  assert.equal(real.ports.worker.sent[0].type, "show");
  const evil = { type: "answer", allow: true, accept: true, text: "EVIL" };
  // Each of these sends an answer as soon as it connects, before the person has
  // done anything in the real panel.
  const second = await fake.open({ window: made, early: [evil] });
  assert.deepEqual(second.ports.worker.sent, [{ type: "ended" }]);
  const forged = await fake.open({ window: made, sender: { id: "someotherextension" }, early: [evil] });
  assert.deepEqual(forged.ports.worker.sent, [{ type: "ended" }]);
  const elsewhere = await fake.open({ window: made, sender: { url: addressOf("confirm.html") }, early: [evil] });
  assert.deepEqual(elsewhere.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(dialog.done, false, "a page that was not the panel answered");
  await tick(ALLOW_DELAY_MS);
  real.type("answer", "Grace");
  await real.click("dismiss");
  await dialog.promise;
  assert.equal(dialog.error, undefined);
  assert.equal(dialog.value.accept, false, "the answer of another page was taken");
  assert.equal(dialog.value.text, "");
});

// A panel that takes a missing choice for an accepted dialog returns a value
// here, where the dialog is to stay open.
test("an answer that the panel's own page sent before it was shown gets a token from the shared window and no choice from the worker, so the dialog stays open and is not accepted for the person", async () => {
  const { fake, windows } = started({ opening: false });
  const action = await asked(windows, fake);
  await fake.open({ window: fake.created[0] });
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await action.promise;
  const dialog = await askedDialog(windows, action.resolution);
  await fake.open({
    window: fake.created.at(-1),
    early: [{ type: "answer", allow: true, accept: true, text: "EARLY" }],
  });
  await dialog.promise;
  assert.equal(dialog.value, undefined, "an answer nobody gave was taken");
  assert.ok(dialog.error instanceof Failure && dialog.error.code === "dialog_open", String(dialog.error?.code));
});

test("the panel counts down to the time left of the call, which is counted from the first window, and the dialog stays open when the time is up", async () => {
  const { fake, windows } = started();
  const action = await asked(windows, fake);
  await tick(20_000);
  await fake.page.click("allow");
  await action.promise;
  const dialog = await askedDialog(windows, action.resolution);
  const panel = fake.page;
  assert.equal(plain(panel, "wait"), "The dialog stays open in the page if you do not answer in 40 seconds.");
  await tick(39_999);
  assert.equal(dialog.done, false);
  await tick(1);
  await dialog.promise;
  assert.ok(dialog.error instanceof Failure);
  assert.equal(dialog.error.code, "dialog_open");
  assert.deepEqual(errorOf(dialog.error), { kind: "failed", message: SENTENCES.dialog_open() });
  assert.deepEqual(fake.windowsOpen(), [], "the panel stayed open");
});

test("a dialog nobody answers is left open and fails the call with one sentence, however the panel ends, and a dialog asked about after the call's time asks nothing", async () => {
  const ends = {
    "the panel is closed": (fake, panel) => fake.close(panel.opened.id),
    "the port is closed": (fake, panel) => panel.ports.page.disconnect(),
    "no answer is given": () => tick(CONFIRMATION_WAIT_MS),
  };
  for (const [way, end] of Object.entries(ends)) {
    const { fake, windows } = started();
    const action = await asked(windows, fake);
    await fake.page.click("deny");
    await action.promise;
    const dialog = await askedDialog(windows, action.resolution);
    const panel = fake.pages.findLast((page) => page.file === "dialog.html");
    assert.ok(panel, way);
    await end(fake, panel);
    await dialog.promise;
    assert.ok(dialog.error instanceof Failure && dialog.error.code === "dialog_open", way);
    assert.deepEqual(fake.windowsOpen(), [], `${way} left the panel open`);
  }
  const { fake, windows } = started();
  const first = Object.freeze({ description: ACTION });
  const early = following(windows.ask(first));
  await flush();
  await tick(CONFIRMATION_WAIT_MS);
  await early.promise;
  const before = created(fake).length;
  const late = await askedDialog(windows, first);
  assert.equal(late.error.code, "dialog_open");
  assert.equal(created(fake).length, before, "a panel was opened with no time left");
  const unknown = following(windows.askDialog(DIALOG, { ...WHERE, first: {} }));
  await flush();
  assert.equal(unknown.error.code, "dialog_open");
  assert.equal(created(fake).length, before);
});

test("a panel that cannot be opened leaves the dialog open with the same sentence, and a page of the panel that opens afterwards is sent away", async () => {
  const { fake, windows } = started({ opening: false });
  const action = await asked(windows, fake);
  await fake.open({ window: fake.created[0] });
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await action.promise;
  fake.chrome.windows.create = async () => {
    throw new Error("No current window");
  };
  const failed = await askedDialog(windows, action.resolution);
  assert.equal(failed.error.code, "dialog_open");
  // A page of the panel that opens afterwards finds nobody waiting.
  const stray = await fake.open({ file: "dialog.html", window: { id: 90, tabs: [{ id: 900 }] } });
  assert.deepEqual(stray.ports.worker.sent, [{ type: "ended" }]);
});

test("windows an earlier worker left open are closed for both pages, and no other page of the extension", async () => {
  const { fake, windows } = started();
  fake.contexts.push(
    { contextType: "TAB", documentUrl: addressOf("confirm.html"), windowId: 55 },
    { contextType: "TAB", documentUrl: addressOf("dialog.html"), windowId: 56 },
    { contextType: "TAB", documentUrl: addressOf("popup.html"), windowId: 57 },
    { contextType: "TAB", documentUrl: `${addressOf("dialog.html")}?again`, windowId: 58 },
    { contextType: "TAB", documentUrl: `${addressOf("dialog.html")}x`, windowId: 59 },
  );
  await windows.closeLeftovers();
  assert.deepEqual(
    fake.calls.filter(([call]) => call === "windows.remove").map(([, id]) => id),
    [55, 56, 58],
  );
});

test("a port that is not the shared window's port, from no known page, is left alone", async () => {
  const { fake } = started({ opening: false });
  const other = await fake.open({
    file: "dialog.html",
    window: { id: 80, tabs: [{ id: 800 }] },
    name: "something-else",
    early: [{ type: "answer", allow: true }],
  });
  assert.deepEqual(other.ports.worker.sent, []);
  assert.equal(other.closed, false);
  assert.ok(EXTENSION);
});

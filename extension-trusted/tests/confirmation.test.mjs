// The confirmation window of the BraveBot extension's tests, run against the
// copies this extension holds of the shared files: the service worker that opens
// the window and waits (the real confirmation.js) and the page the person answers
// in (the real confirm.js), over the fake browser and page of fake-window.mjs.
// The tests act as the person, pressing the buttons of the page and closing its
// window, and as a web page or another context trying to answer for them. What
// differs from the BraveBot extension's own tests is the actions the window words
// (actions.js, written for this extension), the name in the heading, and what the
// manifest declares.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, mock, test } from "node:test";
import {
  ALLOW_DELAY_MS,
  CONFIRMATION_PORT,
  CONFIRMATION_WAIT_MS,
  HEARTBEAT_MS,
  spend,
} from "../approval.js";
import { ACTIONS, CARRIED, describe as describeAction, ontoText } from "../actions.js";
import { BEFORE_WINDOW_MS, CALL_WAIT_MS, DETACH_WAIT_MS } from "../cdp.js";
import { CLOSE_WAIT_MS } from "../confirmation.js";
import { Refusal, ToolError } from "../errors.js";
import { FIRST_ID } from "../pair.js";
import {
  EXCERPT_LIMIT,
  NAME_LIMIT,
  TARGET_LIMIT,
  TYPED_TEXT_LIMIT,
  URL_LIMIT,
} from "../words.js";
import { WAIT_MS } from "../../extension/trusted.js";
import { ADDRESS, EXTENSION, flush, world } from "./fake-window.mjs";
import { scripts } from "./scripts.mjs";

// The name the manifest gives the extension, which the window's heading and title
// repeat.
const NAME = JSON.parse(readFileSync(new URL("../manifest.json", import.meta.url), "utf8")).name;

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] }));
afterEach(() => mock.timers.reset());

const tick = async (ms) => {
  mock.timers.tick(ms);
  await flush();
};

const describe = (more = {}) => ({
  action: "click",
  origin: "https://shop.example",
  url: "https://shop.example/cart",
  tag: "button",
  role: "button",
  name: "Pay now",
  excerpt: "Pay now",
  goes: "",
  rect: { x: 1, y: 2, width: 3, height: 4 },
  ...more,
});

// What the tool that found the element hands the window.
const resolutionOf = (more) =>
  Object.freeze({ tabId: 1, description: Object.freeze(describe(more)) });

// Asks, and follows what the question comes to without waiting for it.
function asking(fake, resolution = resolutionOf()) {
  const state = { done: false, token: undefined, error: undefined };
  state.promise = fake.confirmation.ask(resolution).then(
    (token) => {
      state.done = true;
      state.token = token;
    },
    (error) => {
      state.done = true;
      state.error = error;
    },
  );
  state.resolution = resolution;
  return state;
}

// A question with its window open and its page showing, and the person having
// waited until Allow once is available.
async function shown(options, more) {
  const fake = world({ ...options, page: { name: NAME, ...options?.page } });
  const question = asking(fake, resolutionOf(more));
  await flush();
  return { fake, question, page: fake.page };
}

const text = (page, id) => page.element(id).shown();
const plain = (page, id) => page.element(id).textContent;

test("a window of the extension's own page opens in front, small, and shows the action, the page and the element as text", async () => {
  const { fake, page, question } = await shown();
  assert.deepEqual(fake.calls[0], [
    "windows.create",
    { url: ADDRESS, type: "popup", focused: true, width: 600, height: 640 },
  ]);
  assert.equal(plain(page, "action"), "Click with the mouse, as a person would");
  assert.equal(plain(page, "origin"), "https://shop.example");
  assert.equal(plain(page, "url"), "https://shop.example/cart");
  assert.equal(plain(page, "role"), "button");
  assert.equal(plain(page, "tag"), "button");
  assert.equal(plain(page, "name"), "Pay now");
  assert.equal(plain(page, "excerpt"), "Pay now");
  assert.deepEqual(text(page, "goes"), [{ mark: "nothing" }]);
  assert.equal(page.element("typed-section").hidden, true);
  assert.equal(plain(page, "wait"), "Nothing is done if you do not answer in 60 seconds.");
  assert.equal(question.done, false);
});

// Brave shows "<name>" started debugging this browser, a Cancel button and a
// close button across the top of a popup window while an extension debugs the
// browser. With the name BraveBot Trusted Input the sentence takes about 350 pixels
// and the buttons about 110 (Brave 1.98.63). A window 480 wide cut the sentence to
// `...Bot Trusted Input" started debugging this browser`, and one 560 wide read it
// in full.
const NOTICE_WIDTH = 560;

test("the window is wide enough for the browser's debugging notice to be read in full, and as tall as before", async () => {
  const { fake } = await shown();
  const [, options] = fake.calls[0];
  assert.ok(options.width >= NOTICE_WIDTH, `the window is ${options.width} pixels wide`);
  assert.equal(options.height, 640);
});

test("the heading and the title of the window name this extension, as its manifest does, so that a person who has both extensions can tell whose window is open", async () => {
  const { page } = await shown();
  assert.equal(NAME, "BraveBot Trusted Input");
  assert.equal(plain(page, "heading"), "Allow BraveBot Trusted Input to do this in the page?");
  assert.equal(page.document.title, "BraveBot Trusted Input asks before it acts");
  assert.notEqual(plain(page, "heading"), "Allow BraveBot to do this in the page?");
});

test("a confirmation made for another page opens that page, takes an answer from that page alone and closes the windows of that page", async () => {
  const fake = world({ opening: "none", confirmationPage: "approve.html" });
  const address = `chrome-extension://${EXTENSION}/approve.html`;
  const question = asking(fake);
  await flush();
  assert.equal(fake.calls[0][1].url, address);
  const created = fake.created[0];
  const answer = { type: "answer", allow: true };
  const wrong = await fake.open({ window: created, sender: { url: ADDRESS }, early: [answer] });
  assert.deepEqual(wrong.ports.worker.sent, [{ type: "ended" }], "the default page was taken");
  assert.equal(question.done, false, "the default page answered");
  const real = await fake.open({ window: created });
  assert.equal(real.ports.worker.sent[0].type, "show");
  await tick(ALLOW_DELAY_MS);
  await real.click("allow");
  await question.promise;
  assert.ok(spend(question.token, question.resolution));
  fake.contexts.push(
    { contextType: "TAB", documentUrl: ADDRESS, windowId: 55 },
    { contextType: "TAB", documentUrl: address, windowId: 56 },
  );
  await fake.confirmation.closeLeftovers();
  assert.deepEqual(
    fake.calls.filter(([name]) => name === "windows.remove").map(([, id]) => id),
    [created.id, 56],
  );
});

test("Deny has the focus, and Allow once is not available until a second after the window is shown", async () => {
  const { page } = await shown();
  assert.equal(page.focused, page.element("deny"));
  assert.equal(page.element("allow").disabled, true);
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(page.element("allow").disabled, true);
  await tick(1);
  assert.equal(page.element("allow").disabled, false);
  assert.equal(page.focused, page.element("deny"), "the focus moved to Allow once");
});

test("a click on Allow once before it is available, or in a window that is not in front, does not allow", async () => {
  const { page, question } = await shown();
  await page.click("allow");
  assert.equal(question.done, false, "a click at once allowed");
  await tick(ALLOW_DELAY_MS);
  page.blur();
  assert.equal(page.element("allow").disabled, true, "Allow once stayed available out of front");
  await page.click("allow");
  assert.equal(question.done, false);
  page.focus();
  assert.equal(page.element("allow").disabled, true, "Allow once came back at once");
  await tick(ALLOW_DELAY_MS - 1);
  await page.click("allow");
  assert.equal(question.done, false);
  await tick(1);
  await page.click("allow");
  await question.promise;
  assert.ok(spend(question.token, question.resolution));

  const hidden = await shown({ page: { focused: false } });
  await tick(5 * ALLOW_DELAY_MS);
  assert.equal(hidden.page.element("allow").disabled, true, "a window never in front allowed");
  hidden.page.visible = false;
  hidden.page.focus();
  hidden.page.fireDocument("visibilitychange");
  await tick(5 * ALLOW_DELAY_MS);
  assert.equal(hidden.page.element("allow").disabled, true);
});

test("a click that is not the person's, or that no user gesture made, does not allow", async () => {
  const { page, question } = await shown();
  await tick(ALLOW_DELAY_MS);
  await page.click("allow", { isTrusted: false });
  assert.equal(question.done, false, "a script's click allowed");
  page.active = false;
  await page.click("allow");
  assert.equal(question.done, false, "a click with no user activation allowed");
  page.active = true;
  await page.click("allow");
  await question.promise;
  assert.ok(spend(question.token, question.resolution));
});


// A box that holds more than it shows: a path of 600 characters in a box 288 pixels tall. The fake
// page has the box laid out so, and the person scrolls it.
const SCROLLS = { scrollHeight: 900, clientHeight: 288 };
const END = SCROLLS.scrollHeight - SCROLLS.clientHeight;
const LONG = { action: "upload", file: `report.txt, 12 bytes, /srv/uploads/${"d/".repeat(290)}report.txt` };

test("when the box that holds the path of the file scrolls, Allow once is not available until the box has been scrolled to its end, and the window says why", async () => {
  const { page, question } = await shown({ page: { box: SCROLLS } }, LONG);
  assert.equal(page.focused, page.element("deny"));
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, true, "Allow once came with the box unread");
  assert.match(plain(page, "status"), /not available until the box above has been scrolled to its end/);
  await page.click("allow");
  assert.equal(question.done, false, "a click allowed with the box unread");
  await page.scroll("typed-box", END / 2);
  assert.equal(page.element("allow").disabled, true, "half of the box was taken for all of it");
  await page.scroll("typed-box", END - 2);
  assert.equal(page.element("allow").disabled, true, "a box 2 pixels short of its end was taken for read");
  await page.scroll("typed-box", END);
  assert.equal(page.element("allow").disabled, false);
  assert.equal(plain(page, "status"), "");
  assert.equal(page.focused, page.element("deny"), "the focus moved to Allow once");
  await page.click("allow");
  await question.promise;
  assert.equal(question.error, undefined);
  assert.ok(spend(question.token, question.resolution));
});

test("a box scrolled to its end before the second has passed still waits for the second, and stays read when scrolled back or when the window leaves the front and returns", async () => {
  const { page, question } = await shown({ page: { box: SCROLLS } }, LONG);
  await page.scroll("typed-box", END - 0.5);
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(page.element("allow").disabled, true, "the second was not waited for");
  await tick(1);
  assert.equal(page.element("allow").disabled, false);
  await page.scroll("typed-box", 0);
  assert.equal(page.element("allow").disabled, false, "scrolling back un-read the box");
  page.blur();
  assert.equal(page.element("allow").disabled, true);
  page.focus();
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(page.element("allow").disabled, true);
  await tick(1);
  assert.equal(page.element("allow").disabled, false);
  assert.equal(question.done, false);
});

test("a window made another size starts the reading again: a box that comes to scroll is unread, and one that no longer does is read", async () => {
  const { page } = await shown({}, LONG);
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, false, "a box that holds what it shows was unread");
  Object.assign(page.element("typed-box"), SCROLLS);
  page.fireWindow("resize");
  assert.equal(page.element("allow").disabled, true, "a box that came to scroll was read");
  assert.match(plain(page, "status"), /scrolled to its end/);
  await page.scroll("typed-box", END);
  assert.equal(page.element("allow").disabled, false);
  Object.assign(page.element("typed-box"), { scrollHeight: 400, scrollTop: 0 });
  page.fireWindow("resize");
  assert.equal(page.element("allow").disabled, true, "the reading was not started again");
  Object.assign(page.element("typed-box"), { scrollHeight: 288 });
  page.fireWindow("resize");
  assert.equal(page.element("allow").disabled, false, "a box that no longer scrolls stayed unread");
  assert.equal(plain(page, "status"), "");
});

test("a box that is not laid out is not read, a box that holds what it shows leaves Allow once as it was, and a call with no box ignores the box", async () => {
  const unlaid = await shown({ page: { box: { scrollHeight: 0, clientHeight: 0 } } }, LONG);
  await tick(5 * ALLOW_DELAY_MS);
  assert.equal(unlaid.page.element("allow").disabled, true, "a box with no size was taken for read");
  Object.assign(unlaid.page.element("typed-box"), { scrollHeight: 288, clientHeight: 288 });
  unlaid.page.fireWindow("resize");
  assert.equal(unlaid.page.element("allow").disabled, false);

  // A box 1 pixel taller than it shows is rounding and not more to read.
  const fits = await shown({ page: { box: { scrollHeight: 289, clientHeight: 288 } } }, LONG);
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(fits.page.element("allow").disabled, true);
  await tick(1);
  assert.equal(fits.page.element("allow").disabled, false);

  // The box of a call that carries nothing is hidden, and a hidden box has no size.
  for (const box of [{ scrollHeight: 0, clientHeight: 0 }, SCROLLS]) {
    const none = await shown({ page: { box } });
    assert.equal(none.page.element("typed-section").hidden, true);
    await tick(ALLOW_DELAY_MS);
    assert.equal(none.page.element("allow").disabled, false, "a hidden box held Allow once back");
    assert.equal(plain(none.page, "status"), "");
  }
});

test("Allow once gives a token for the question's own resolution, once, and closes the window", async () => {
  const { fake, page, question } = await shown();
  await tick(ALLOW_DELAY_MS);
  await page.click("allow");
  await question.promise;
  assert.equal(question.error, undefined);
  assert.equal(spend(question.token, question.resolution), true, "the token is not for the resolution");
  assert.equal(spend(question.token, question.resolution), false, "a token was served twice");
  assert.deepEqual(
    fake.calls.filter(([name]) => name === "windows.remove").map(([, id]) => id),
    [page.opened.id],
  );
  assert.deepEqual(fake.windowsOpen(), []);
  assert.equal(plain(page, "status"), "Allowed. This window closes.");
  assert.equal(page.element("allow").disabled, true);
  assert.equal(page.element("deny").disabled, true);

  // A token made for one question does not serve the next, which is a resolution of its own.
  const first = asking(fake);
  await flush();
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await first.promise;
  const second = asking(fake);
  await flush();
  await tick(ALLOW_DELAY_MS);
  await fake.page.click("allow");
  await second.promise;
  assert.equal(spend(first.token, second.resolution), false, "a token served another resolution");
  assert.equal(spend(second.token, second.resolution), true);
});

test("Deny, Escape, a closed window, a closed port and the time limit each deny, say why, and give no token", async () => {
  const ways = {
    "Deny": [(f, p) => p.click("deny"), /you denied the action in the browser/],
    "Escape": [(f, p) => p.key("Escape"), /you denied the action in the browser/],
    "the window closed": [(f, p) => f.close(p.opened.id), /confirmation window was closed/],
    "the port closed": [(f, p) => p.ports.page.disconnect(), /confirmation window was closed/],
    "no answer": [() => tick(CONFIRMATION_WAIT_MS), /within 60 seconds/],
  };
  for (const [way, [act, message]] of Object.entries(ways)) {
    const { fake, page, question } = await shown();
    await tick(ALLOW_DELAY_MS);
    await act(fake, page);
    await flush();
    assert.equal(question.done, true, `${way} did not end the question`);
    await question.promise;
    assert.ok(question.error instanceof Refusal, way);
    assert.match(question.error.message, message, way);
    assert.equal(question.token, undefined, way);
    assert.deepEqual(fake.windowsOpen(), [], `${way} left the window open`);
    // The next question is asked afresh.
    const next = asking(fake);
    await flush();
    assert.equal(next.done, false, way);
    await fake.page.click("deny");
    await next.promise;
  }
});

test("an answer that is not exactly allow true denies", async () => {
  for (const allow of ["true", 1, "yes", {}, undefined]) {
    const { page, question } = await shown();
    page.ports.page.postMessage({ type: "answer", allow });
    await flush();
    await question.promise;
    assert.match(question.error?.message ?? "", /you denied the action/, String(allow));
  }
  const { page, question } = await shown();
  page.ports.page.postMessage({ type: "show" });
  page.ports.page.postMessage({ type: "alive" });
  page.ports.page.postMessage({ type: "anything" });
  page.ports.page.postMessage("allow");
  await flush();
  assert.equal(question.done, false);
});

test("a second question while one waits is refused with a reason, and the first is not disturbed", async () => {
  const { fake, page, question } = await shown();
  const second = asking(fake);
  await flush();
  assert.ok(second.error instanceof Refusal);
  assert.match(second.error.message, /another action is waiting for your answer/);
  assert.equal(fake.calls.filter(([name]) => name === "windows.create").length, 1);
  assert.equal(question.done, false);
  await tick(ALLOW_DELAY_MS);
  await page.click("allow");
  await question.promise;
  assert.ok(spend(question.token, question.resolution));
});

test("a window that cannot be opened is a failure that says so, and gives no token", async () => {
  const fake = world({ failing: new Error("No current window") });
  const question = asking(fake);
  await flush();
  assert.ok(question.error instanceof ToolError);
  assert.ok(!(question.error instanceof Refusal));
  assert.match(question.error.message, /could not be opened: No current window/);
  assert.equal(question.token, undefined);
  const next = asking(fake);
  await flush();
  assert.match(next.error.message, /could not be opened/, "the failed question stayed pending");
});

test("a question that never gets a window denies when the time is up", async () => {
  const fake = world({ opening: "none" });
  const question = asking(fake);
  await flush();
  await tick(CONFIRMATION_WAIT_MS);
  await question.promise;
  assert.match(question.error.message, /within 60 seconds/);
  assert.deepEqual(fake.windowsOpen(), []);
});

// The browser can fail to answer windows.create or windows.remove. The worker's
// call, and the place in line it holds, end all the same.
test("a window the browser never says it opened does not hold the question: it denies when the time is up, the next question is asked, and a window that opens later is sent away", async () => {
  const fake = world({ opening: "never" });
  const question = asking(fake);
  await flush();
  assert.equal(question.done, false);
  await tick(CONFIRMATION_WAIT_MS);
  assert.equal(question.done, true, "the question is held by a window that was never reported open");
  assert.ok(question.error instanceof Refusal);
  assert.match(question.error.message, /within 60 seconds/);
  assert.deepEqual(
    fake.calls.filter(([name]) => name === "windows.remove"),
    [],
    "a window that was not reported open was removed by an id nobody knows",
  );

  // The place in line is free: the next question is asked, and is not refused as a second.
  const next = asking(fake);
  await flush();
  assert.equal(next.done, false);
  assert.equal(fake.calls.filter(([name]) => name === "windows.create").length, 2);
  await tick(CONFIRMATION_WAIT_MS);
  assert.match(next.error.message, /within 60 seconds/);

  // The first window opens at last: no question is waiting for it, so it is sent away and ends.
  const late = fake.created[0];
  fake.openLater();
  const page = await fake.open({ window: late });
  assert.deepEqual(page.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(page.closed, true);
});

test("a browser that does not answer the closing of the window holds the end of the question for CLOSE_WAIT_MS and no longer, whether the person allowed or denied", async () => {
  assert.ok(CLOSE_WAIT_MS > 0 && CLOSE_WAIT_MS <= 2_000, `${CLOSE_WAIT_MS} ms`);
  for (const [way, answer, check] of [
    ["Deny", "deny", (q) => assert.match(q.error?.message ?? "", /you denied the action/)],
    ["Allow once", "allow", (q) => assert.ok(spend(q.token, q.resolution), "no token")],
  ]) {
    const { fake, page, question } = await shown({ closing: "never" });
    await tick(ALLOW_DELAY_MS);
    await page.click(answer);
    assert.equal(
      fake.calls.filter(([name]) => name === "windows.remove").length,
      1,
      `${way}: the window was not asked to close`,
    );
    await tick(CLOSE_WAIT_MS - 1);
    assert.equal(question.done, false, `${way}: the end came before the browser had time to close`);
    await tick(1);
    assert.equal(question.done, true, `${way}: the end is held by a browser that does not answer`);
    check(question);
    // The place in line is free again.
    const next = asking(fake);
    await flush();
    assert.equal(next.done, false, way);
    assert.equal(next.error, undefined, `${way}: the next question was refused`);
  }
});

test("the time to close the window fits in the wait of the BraveBot extension with the other bounds of a call", () => {
  const worst = BEFORE_WINDOW_MS + CONFIRMATION_WAIT_MS + CLOSE_WAIT_MS + CALL_WAIT_MS + DETACH_WAIT_MS;
  assert.ok(worst < WAIT_MS, `a call can take ${worst} ms of the ${WAIT_MS} the other extension waits`);
});

test("only the page of the window opened for the question can answer it", async () => {
  const fake = world({ opening: "none" });
  const question = asking(fake);
  await flush();
  const created = fake.created[0];
  const answer = { type: "answer", allow: true };
  const forged = {
    "another extension": { id: "someotherextension" },
    "the options page": { url: `chrome-extension://${EXTENSION}/options.html` },
    "a web page": { url: "https://shop.example/cart" },
    "a frame of the window": { frameId: 4 },
    "another window": { tab: { id: 1, windowId: created.id + 1 } },
    "no tab": { tab: undefined },
    "no url": { url: undefined },
  };
  for (const [who, sender] of Object.entries(forged)) {
    const page = await fake.open({ window: created, sender, early: [answer] });
    assert.deepEqual(page.ports.worker.sent, [{ type: "ended" }], who);
    assert.equal(question.done, false, `${who} answered`);
  }
  const named = await fake.open({ window: created, name: "something-else", early: [answer] });
  assert.equal(question.done, false, "a port with another name answered");
  assert.deepEqual(named.ports.worker.sent, [], "a port with another name was answered");

  const real = await fake.open({ window: created });
  assert.equal(real.ports.worker.sent[0].type, "show");
  const second = await fake.open({ window: created, early: [answer] });
  // confirmation.js checks `state.port === null` twice for a port: before it asks
  // whether the port is the window's and again when that is answered. Only the
  // second decides, so a worker without the first refuses a second port the same
  // way, and no test can tell the two apart.
  assert.deepEqual(second.ports.worker.sent, [{ type: "ended" }], "a second port was taken");
  assert.equal(question.done, false, "a second port answered");
  await tick(ALLOW_DELAY_MS);
  await real.click("allow");
  await question.promise;
  assert.ok(spend(question.token, question.resolution));
});

test("a window with no question waiting is sent away, and its answer does nothing", async () => {
  const fake = world({ opening: "none" });
  const page = await fake.open({
    window: { id: 41, tabs: [{ id: 410 }] },
    early: [{ type: "answer", allow: true }],
  });
  assert.deepEqual(page.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(page.closed, true);
  const question = asking(fake);
  await flush();
  assert.equal(question.done, false, "an answer from before the question allowed it");
});

test("a worker that stops during the wait leaves a denial: the window ends itself, and a window from before cannot answer the next question", async () => {
  const { fake, page, question } = await shown();
  await tick(ALLOW_DELAY_MS);
  const sent = page.ports.page.sent.length;
  // The browser closes the worker's ports when the worker stops.
  page.ports.worker.disconnect();
  await flush();
  assert.equal(page.closed, true, "the window stayed open");
  assert.equal(page.element("allow").disabled, true);
  assert.equal(page.element("deny").disabled, true);
  assert.match(plain(page, "status"), /no longer waiting for this window, so nothing was done/);
  await page.click("allow");
  await page.key("Escape");
  assert.equal(page.ports.page.sent.length, sent, "the window sent something after the port closed");

  // A new worker has no question until one is asked, and then only its own window answers.
  const fresh = world({ opening: "none" });
  const next = asking(fresh);
  await flush();
  const stale = await fresh.open({
    window: { id: 5, tabs: [{ id: 1 }] },
    early: [{ type: "answer", allow: true }],
  });
  assert.deepEqual(stale.ports.worker.sent, [{ type: "ended" }]);
  assert.equal(next.done, false);
  assert.equal(question.token, undefined);
  await tick(CONFIRMATION_WAIT_MS);
  await next.promise;
  assert.match(next.error.message, /within 60 seconds/);
});

test("the window tells the worker it is still there while it waits, and stops when it is answered", async () => {
  const { page, question } = await shown();
  const alive = () => page.ports.page.sent.filter((message) => message.type === "alive").length;
  await tick(HEARTBEAT_MS - 1000);
  assert.equal(alive(), 0);
  await tick(1000);
  assert.equal(alive(), 1);
  await tick(HEARTBEAT_MS);
  assert.equal(alive(), 2);
  await page.click("deny");
  await question.promise;
  const counted = plain(page, "wait");
  await tick(3 * HEARTBEAT_MS);
  assert.equal(alive(), 2, "the window kept sending after it was answered");
  assert.equal(plain(page, "wait"), counted, "the window went on counting after it was answered");
});

test("the window counts down to the time the worker denies at", async () => {
  const { page } = await shown();
  await tick(10_000);
  assert.equal(plain(page, "wait"), "Nothing is done if you do not answer in 50 seconds.");
  await tick(50_000);
  assert.equal(plain(page, "wait"), "The time is up. Nothing is done.");
  assert.ok(CONFIRMATION_WAIT_MS > 2 * HEARTBEAT_MS);
});

// What a page is able to put in a word: markup, a line break, an override that
// draws the end of a word first, a character with no width, a space that is not
// one. The window shows each as the text it is.
test("markup in a name is text, and control, direction and invisible characters are marked with their code points", async () => {
  const markup = `<img src=x onerror=alert(1)><script>alert(2)</script>`;
  const typed = "first\nsecond‮evil\u0007 ​ end";
  const { page } = await shown(
    {},
    {
      action: "type",
      text: typed,
      name: markup,
      tag: "x-evil‮",
      excerpt: "a b",
      goes: "https://evil.example/‮gpj.exe",
      url: "https://shop.example/\u0000",
    },
  );
  assert.deepEqual(text(page, "name"), [markup]);
  assert.deepEqual(text(page, "typed"), [
    "first",
    { mark: "U+000A" },
    "second",
    { mark: "U+202E" },
    "evil",
    { mark: "U+0007" },
    " ",
    { mark: "U+200B" },
    { mark: "U+00A0" },
    "end",
  ]);
  assert.deepEqual(text(page, "tag"), ["x-evil", { mark: "U+202E" }]);
  assert.deepEqual(text(page, "excerpt"), ["a", { mark: "U+2029" }, "b"]);
  assert.deepEqual(text(page, "goes"), [
    "https://evil.example/",
    { mark: "U+202E" },
    "gpj.exe",
  ]);
  assert.deepEqual(text(page, "url"), ["https://shop.example/", { mark: "U+0000" }]);
});

// A combining mark is drawn on the character before it, and a word of base letters
// and marks stacks them over the rows around it. A few after a letter are text; the
// rest are marked, and a row draws inside its own box.
test("a combining mark after a letter is shown up to three times, and every other one is marked with its code point, so a word of letters and marks cannot stack over the rows around it", async () => {
  const mark = "\u0316";
  const flood = "C" + mark.repeat(200);
  const { page } = await shown(
    {},
    {
      name: flood,
      excerpt: "cafe\u0301 \u1ec7 e\u0323\u0302 \u0e01\u0e34\u0e49 \u0645\u0651\u064e",
      tag: `x${"\u0301".repeat(3)}${"\u0301"}`,
      goes: ` \u0301ab \u0301${"\u200b"}\u0301 a\u034fb`,
    },
  );
  // Three marks stay with the letter, the other 196 are marked, and the word is
  // cut at its limit, which counts the marks.
  const name = text(page, "name");
  assert.equal(name[0], `C${mark.repeat(3)}`);
  assert.equal(name.length, 1 + 196 + 1);
  assert.deepEqual(name.slice(1, -1), Array.from({ length: 196 }, () => ({ mark: "U+0316" })));
  assert.deepEqual(name.at(-1), { mark: "1 more characters not shown" });
  // Accented, Vietnamese, Thai and Arabic text keeps its marks.
  assert.deepEqual(text(page, "excerpt"), ["cafe\u0301 \u1ec7 e\u0323\u0302 \u0e01\u0e34\u0e49 \u0645\u0651\u064e"]);
  // A fourth mark is marked.
  assert.deepEqual(text(page, "tag"), [`x${"\u0301".repeat(3)}`, { mark: "U+0301" }]);
  // A mark at the start, after a space, after a character that was marked, and
  // the combining grapheme joiner, which draws nothing, are marked.
  assert.deepEqual(text(page, "goes"), [
    " ",
    { mark: "U+0301" },
    "ab ",
    { mark: "U+0301" },
    { mark: "U+200B" },
    { mark: "U+0301" },
    " a",
    { mark: "U+034F" },
    "b",
  ]);
});

test("every row of the page's words is cut off at its own box, so the characters of one row cannot be drawn over another", () => {
  const rule = source("confirm.css").match(/(?:^|\n)\.words\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(rule, /overflow:\s*hidden\s*;/);
  for (const id of ["origin", "url", "role", "tag", "name", "excerpt", "goes"]) {
    assert.match(
      source("confirm.html"),
      new RegExp(`<dd id="${id}" class="words"></dd>`),
      `${id} is a row that is cut off at its box`,
    );
  }
});

test("the destination comes before the role, tag, name and text, which are each a box of at most five lines that scrolls and shows its scroll bar, so a word of marks cannot push the destination or the buttons out of view", () => {
  const page = source("confirm.html");
  const at = (id) => page.indexOf(`id="${id}"`);
  for (const id of ["role", "tag", "name", "excerpt"]) {
    assert.ok(at("goes") > 0 && at("goes") < at(id), `the destination is not above ${id}`);
  }
  // The address and the site are above the destination and the element rows are
  // below it, and the text a call types is below all of them, so that none of
  // its lines is between the top of the window and the destination.
  assert.ok(at("origin") < at("url") && at("url") < at("goes"), "the page is below the destination");
  assert.ok(at("excerpt") < at("typed-section"), "the typed text is above the element rows");
  assert.ok(at("typed-section") < page.indexOf('<div class="bar">'), "the typed text is below the bar");
  const css = source("confirm.css");
  const rule = css.match(/(?:^|\n)dd\.words\s*\{([^}]*)\}/)?.[1] ?? "";
  const height = Number(rule.match(/max-height:\s*([\d.]+)em\s*;/)?.[1]);
  assert.ok(height > 0 && height <= 6, `a row is not at most six lines tall (${height})`);
  assert.match(rule, /overflow-y:\s*auto\s*;/);
  // A scroll bar the platform shows only while it is used would leave nothing to
  // say that a row holds more than it shows.
  assert.match(css, /dd\.words::-webkit-scrollbar[^{]*\{[^}]*width:/);
});

test("the site and the address are boxes taller than five lines, so the longest address is not behind a scroll bar, and they are not so tall that a row of marks pushes the destination out of view", () => {
  const css = source("confirm.css");
  const heightOf = (selector) => {
    const rule = css.match(new RegExp(`(?:^|\\n)${selector}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
    return Number(rule.match(/max-height:\s*([\d.]+)em\s*;/)?.[1]);
  };
  const rows = heightOf("dd\\.words");
  // 300 characters of the widest letter take 142 pixels at 600 pixels wide in a
  // font of 14 pixels, which is 10.2em, and a row of marks takes 499 pixels.
  const address = heightOf("dd#origin,\\s*dd#url");
  assert.ok(address >= 11, `the box of the address is ${address}em tall`);
  assert.ok(address <= 16, `the box of the address is ${address}em tall`);
  assert.ok(address > rows, "the address is in a box no taller than the rows of the element");
  assert.match(source("confirm.html"), /<dd id="origin" class="words"><\/dd>/);
  assert.match(source("confirm.html"), /<dd id="url" class="words"><\/dd>/);
});

test("where a click goes is shown as the window's own word when it cannot be told, a page's word of the same letters is text, and a note that is not a plain yes cannot be shown", async () => {
  const { page } = await shown({}, { goes: "", goesUnknown: true });
  assert.deepEqual(text(page, "goes"), [{ mark: "unknown" }]);
  const own = await shown({}, { goes: "unknown" });
  assert.deepEqual(text(own.page, "goes"), ["unknown"], "a page's word is text, not the window's mark");
  const nothing = await shown({}, { goes: "" });
  assert.deepEqual(text(nothing.page, "goes"), [{ mark: "nothing" }]);
  for (const note of [false, "yes", 1, null, {}]) {
    const refused = await shown({}, { goes: "", goesUnknown: note });
    assert.match(plain(refused.page, "status"), /^This request cannot be shown, so it cannot be allowed/, String(note));
    assert.equal(refused.page.element("allow").disabled, true, String(note));
  }
});

test("each action is named in the window's own words, and typed text, a key, the element a drag drops onto and the size of a page are shown in a frame of their own", async () => {
  const onto = ontoText({ role: "button", tag: "div", name: "Trash", excerpt: "Drop here", goes: "" });
  const cases = [
    ["click", undefined, /^Click with the mouse, as a person would$/, true],
    ["double_click", undefined, /^Double-click with the mouse$/, true],
    ["right_click", undefined, /^Right-click with the mouse, which opens the page's context menu$/, true],
    ["type", { text: "hello" }, /^Type this text on the keyboard, in place of what the field holds$/, false],
    ["key", { key: "Enter" }, /^Press this key on the keyboard, on the element that has the focus$/, false],
    ["hover", undefined, /^Move the mouse pointer over this element, without clicking$/, true],
    ["drag", { onto }, /^Drag the element listed under The element with the mouse, and drop it onto the element in the frame$/, false],
    ["upload", { file: "report.txt, 12 bytes, /srv/uploads/report.txt" }, /^Choose the file listed under File on the file input listed under The element\. The browser reads the file from your computer itself and gives it to the page, as if you had chosen it in the file picker, and a page whose script sends a file when it is chosen receives it as soon as you press Allow once\. No bytes of the file pass through BraveBot$/, false],
    ["upload_hidden", { file: "report.txt, 12 bytes, /srv/uploads/report.txt" }, /^Choose the file listed under File on the file input listed under The element\. The page hides this input, so you cannot see it in the page, and the name below is what the page's own label, aria-label or title calls it\. The browser reads the file from your computer itself and gives it to the page, as if you had chosen it in the file picker, and a page whose script sends a file when it is chosen receives it as soon as you press Allow once\. No bytes of the file pass through BraveBot$/, false],
    ["upload_hidden_id", { file: "report.txt, 12 bytes, /srv/uploads/report.txt" }, /^Choose the file listed under File on the file input listed under The element\. The page hides this input and gives it no label, so the name below is the id or name of the input in the page's markup, which says little of what it is for\. The browser reads the file from your computer itself and gives it to the page, as if you had chosen it in the file picker, and a page whose script sends a file when it is chosen receives it as soon as you press Allow once\. No bytes of the file pass through BraveBot$/, false],
    ["picture", { size: "1280 by 6040 CSS pixels" }, /^Take a picture of the whole page, including the parts out of view .* the browser's debugger is attached to the page, and it reads everything on the page\. No element is acted on$/, false],
    ["console", { limits: "errors, the newest 20 at most" }, /^Read what the page has written to its console .* the browser's debugger is attached to the page, and it reads the page's console, which can hold anything the page's scripts printed\. No element is acted on$/, false],
  ];
  assert.deepEqual(Object.keys(ACTIONS), cases.map(([action]) => action));
  for (const [action, carried, words, none] of cases) {
    const { page } = await shown({}, { action, ...carried });
    assert.match(plain(page, "action"), words, action);
    assert.equal(page.element("typed-section").hidden, none, action);
    if (carried) {
      const [value] = Object.values(carried);
      assert.deepEqual(text(page, "typed"), [value], action);
      // A size and what a console read takes are not text typed, so their
      // characters are not counted.
      const counted = !["picture", "console", "upload", "upload_hidden", "upload_hidden_id"].includes(action);
      assert.equal(page.element("typed-count").hidden, !counted, action);
      assert.equal(plain(page, "typed-count"), counted ? `${value.length} characters` : "", action);
    }
  }
  const headings = {
    type: ["Text to type", { text: "a" }],
    key: ["Key to press", { key: "Tab" }],
    drag: ["Drop it onto this element", { onto }],
    upload: ["File: name, size and full path", { file: "a.txt, 1 bytes, /a.txt" }],
    upload_hidden: ["File: name, size and full path", { file: "a.txt, 1 bytes, /a.txt" }],
    upload_hidden_id: ["File: name, size and full path", { file: "a.txt, 1 bytes, /a.txt" }],
    picture: ["Size of the page", { size: "1 by 1 CSS pixels" }],
    console: ["What is read", { limits: "every entry, the newest 100 at most" }],
  };
  for (const [action, [heading, carried]] of Object.entries(headings)) {
    const { page } = await shown({}, { action, ...carried });
    assert.equal(plain(page, "typed-heading"), heading, action);
  }
  assert.deepEqual(Object.keys(CARRIED), [
    "type",
    "key",
    "drag",
    "upload",
    "upload_hidden",
    "upload_hidden_id",
    "picture",
    "console",
  ]);
});

test("a picture of a page is asked about with the site, the address and the size of the page, and the window has no element to show for it", async () => {
  const description = describeAction("picture", {
    origin: "https://shop.example",
    url: "https://shop.example/cart",
    size: "1280 by 6040 CSS pixels",
  });
  const { page } = await shown({}, description);
  assert.match(plain(page, "action"), /^Take a picture of the whole page/);
  assert.equal(plain(page, "origin"), "https://shop.example");
  assert.equal(plain(page, "url"), "https://shop.example/cart");
  assert.equal(plain(page, "typed-heading"), "Size of the page");
  assert.deepEqual(text(page, "typed"), ["1280 by 6040 CSS pixels"]);
  // The heading, the rows and the note about the outline are one block, and it is
  // not on the page. What the action reads is said in its words.
  assert.equal(page.element("element-section").hidden, true);
  assert.match(plain(page, "action"), /the browser's debugger is attached to the page, and it reads everything on the page/);
  assert.equal(page.element("typed-count").hidden, true);
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, false);
});

test("a console read is asked about with the site, the address and what is read, and the window has no element to show for it", async () => {
  const description = describeAction("console", {
    origin: "https://shop.example",
    url: "https://shop.example/cart",
    limits: "errors, the newest 20 at most",
  });
  const { page } = await shown({}, description);
  assert.match(plain(page, "action"), /^Read what the page has written to its console/);
  assert.equal(plain(page, "origin"), "https://shop.example");
  assert.equal(plain(page, "url"), "https://shop.example/cart");
  assert.equal(plain(page, "typed-heading"), "What is read");
  assert.deepEqual(text(page, "typed"), ["errors, the newest 20 at most"]);
  // The heading, the rows and the note about the outline are not on the page, and
  // the words say that the debugger reads the console.
  assert.equal(page.element("element-section").hidden, true);
  assert.match(plain(page, "action"), /the browser's debugger is attached to the page, and it reads the page's console/);
  assert.match(plain(page, "action"), /No element is acted on$/);
  assert.equal(page.element("typed-count").hidden, true);
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, false);
});

test("an action that acts on an element shows the element block and the count of the characters it carries, and an action that acts on none shows neither, for every action of the table", async () => {
  assert.ok(Object.isFrozen(ACTIONS));
  const onto = ontoText({ role: "button", tag: "div", name: "Trash", excerpt: "Drop here", goes: "" });
  const carried = {
    text: "hello",
    key: "Enter",
    onto,
    file: "a.txt, 1 bytes, /a.txt",
    size: "1 by 1 CSS pixels",
    limits: "every entry, the newest 100 at most",
  };
  const without = [];
  for (const [action, row] of Object.entries(ACTIONS)) {
    const { page } = await shown({}, { action, ...carried });
    // The window reads the table: the block is not on the page for an action
    // whose row says it acts on no element, and is for every other.
    assert.equal(page.element("element-section").hidden, !row.element, action);
    if (row.element) {
      assert.equal(plain(page, "name"), "Pay now", action);
    } else {
      without.push(action);
    }
    // A value that is not text typed has no count of characters.
    assert.equal(page.element("typed-count").hidden, CARRIED[action]?.[2] === false, action);
  }
  assert.deepEqual(without, ["picture", "console"]);
  // A description that names an element is shown with it for any action whose
  // row says it acts on one, and the block is not decided by what the description
  // holds.
  const empty = await shown({}, { action: "click", tag: "", role: null, name: "", excerpt: "", goes: "" });
  assert.equal(empty.page.element("element-section").hidden, false);
  assert.deepEqual(text(empty.page, "name"), [{ mark: "nothing" }]);
});

test("a drag shows both elements: the one it takes in the rows of the element and the one it drops onto in the frame, each as the page's words with the characters that cannot be seen marked", async () => {
  const source = { tag: "li", role: "option", name: "Item 3", excerpt: "Third", goes: "" };
  const destination = {
    tag: "x-evil\u202e",
    role: "button",
    name: 'Trash" , goes to "https://good.example',
    excerpt: "a\nb",
    goes: "https://evil.example/\u202egpj",
  };
  const description = describeAction("drag", {
    origin: "https://shop.example",
    url: "https://shop.example/list",
    element: source,
    destination,
  });
  const { page } = await shown({}, description);
  assert.equal(plain(page, "name"), "Item 3");
  assert.equal(plain(page, "tag"), "li");
  const frame = text(page, "typed");
  assert.deepEqual(frame, [
    'role "button", tag "x-evil',
    { mark: "U+202E" },
    '", name "Trash\\" , goes to \\"https://good.example", text "a',
    { mark: "U+000A" },
    'b", goes to "https://evil.example/',
    { mark: "U+202E" },
    'gpj"',
  ]);
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, false);
});

test("the page's words are cut to the limits and say how much was left out, and typed text is shown in full", async () => {
  const long = (limit) => "w".repeat(limit + 50);
  const { page } = await shown(
    {},
    {
      action: "type",
      text: "t".repeat(TYPED_TEXT_LIMIT),
      name: long(NAME_LIMIT),
      tag: long(NAME_LIMIT),
      excerpt: long(EXCERPT_LIMIT),
      goes: long(TARGET_LIMIT),
      url: long(URL_LIMIT),
    },
  );
  for (const [id, limit] of [
    ["name", NAME_LIMIT],
    ["tag", NAME_LIMIT],
    ["excerpt", EXCERPT_LIMIT],
    ["goes", TARGET_LIMIT],
    ["url", URL_LIMIT],
  ]) {
    assert.deepEqual(text(page, id), ["w".repeat(limit), { mark: "50 more characters not shown" }], id);
  }
  assert.deepEqual(text(page, "typed"), ["t".repeat(TYPED_TEXT_LIMIT)]);
  assert.equal(plain(page, "typed-count"), `${TYPED_TEXT_LIMIT} characters`);
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("allow").disabled, false);
});

test("a description the window cannot show in full cannot be allowed", async () => {
  const bad = {
    "text too long to show": { action: "type", text: "t".repeat(TYPED_TEXT_LIMIT + 1) },
    "no text to type": { action: "type" },
    "no element to drop onto": { action: "drag", onto: "" },
    "no key": { action: "key" },
    "an action it does not know": { action: "scroll" },
    "a name that is not text": { name: 5 },
    "an address that is not text": { url: undefined },
    "a role that is not text": { role: 5 },
  };
  for (const [why, more] of Object.entries(bad)) {
    const { page, question } = await shown({}, more);
    await tick(5 * ALLOW_DELAY_MS);
    assert.equal(page.element("allow").disabled, true, why);
    await page.click("allow");
    assert.equal(question.done, false, `${why} was allowed`);
    assert.match(plain(page, "status"), /cannot be shown, so it cannot be allowed/, why);
    assert.equal(page.ports.page.sent.some((message) => message.type === "answer"), false, why);
    await tick(CONFIRMATION_WAIT_MS);
    assert.match(question.error?.message ?? "", /within 60 seconds/, why);
  }
  const none = await shown({}, { role: null });
  assert.deepEqual(text(none.page, "role"), [{ mark: "nothing" }]);
});

test("an element named by its number in a list is shown with that number and which of the elements of its role and name it is, and an element named otherwise shows neither", async () => {
  const plainly = await shown();
  assert.equal(plainly.page.element("number").hidden, true);
  assert.equal(plainly.page.element("number-term").hidden, true);

  const second = await shown({}, { number: 2, position: { index: 2, of: 3 } });
  assert.equal(second.page.element("number").hidden, false);
  assert.equal(second.page.element("number-term").hidden, false);
  assert.equal(plain(second.page, "number"), "2 in the last list, 2 of 3 with this role and name");
  // Two elements that look alike are told apart by the line.
  const first = await shown({}, { number: 1, position: { index: 1, of: 3 } });
  assert.equal(plain(first.page, "number"), "1 in the last list, 1 of 3 with this role and name");
  const unplaced = await shown({}, { number: 7 });
  assert.equal(plain(unplaced.page, "number"), "7 in the last list");
  // The element is still allowed once the person has waited.
  await tick(ALLOW_DELAY_MS);
  assert.equal(second.page.element("allow").disabled, false);
});

test("a number or a place among elements that is not whole numbers cannot be shown, so it cannot be allowed", async () => {
  const bad = {
    "a number of zero": { number: 0 },
    "a negative number": { number: -1 },
    "a number with a fraction": { number: 1.5 },
    "a number that is text": { number: "2" },
    "a place with no number": { position: { index: 1, of: 2 } },
    "a place that is not a pair": { number: 1, position: "1 of 2" },
    "a place of zero": { number: 1, position: { index: 0, of: 2 } },
    "a place past the count": { number: 1, position: { index: 3, of: 2 } },
    "a count that is text": { number: 1, position: { index: 1, of: "2" } },
    "a place that is null": { number: 1, position: null },
  };
  for (const [why, more] of Object.entries(bad)) {
    const { page, question } = await shown({}, more);
    await tick(5 * ALLOW_DELAY_MS);
    assert.equal(page.element("allow").disabled, true, why);
    await page.click("allow");
    assert.equal(question.done, false, `${why} was allowed`);
    assert.match(plain(page, "status"), /cannot be shown, so it cannot be allowed/, why);
    await tick(CONFIRMATION_WAIT_MS);
  }
});

test("windows an earlier worker left open are closed as this one starts, and no other page of the extension", async () => {
  const fake = world();
  fake.contexts.push(
    { contextType: "TAB", documentUrl: ADDRESS, windowId: 55 },
    { contextType: "TAB", documentUrl: `chrome-extension://${EXTENSION}/options.html`, windowId: 56 },
    { contextType: "TAB", documentUrl: `${ADDRESS}?again`, windowId: 57 },
    { contextType: "TAB", documentUrl: ADDRESS, windowId: -1 },
    { contextType: "TAB", documentUrl: `${ADDRESS}x`, windowId: 58 },
  );
  await fake.confirmation.closeLeftovers();
  assert.deepEqual(fake.calls[0], ["runtime.getContexts", { contextTypes: ["TAB"] }]);
  assert.deepEqual(
    fake.calls.filter(([name]) => name === "windows.remove").map(([, id]) => id),
    [55, 57],
  );
  const failing = world();
  failing.chrome.runtime.getContexts = async () => {
    throw new Error("not a function");
  };
  await failing.confirmation.closeLeftovers();
});

const source = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");

// What can turn a string into markup or code. The window's script never uses
// any of it: the page's words reach the window only as text.
const MARKUP = [
  [/\binnerHTML\b/, "innerHTML"],
  [/\bouterHTML\b/, "outerHTML"],
  [/insertAdjacentHTML/, "insertAdjacentHTML"],
  [/document\s*\.\s*write/, "document.write"],
  [/\beval\b/, "eval"],
  [/\bFunction\b/, "Function"],
  [/\bsetTimeout\s*\(\s*["'`]|\bsetInterval\s*\(\s*["'`]/, "a timer given a string"],
  [/createContextualFragment|DOMParser|\bsrcdoc\b|\bimportScripts\b/, "an HTML or code parser"],
  [/\bimport\s*\(/, "a dynamic import"],
];

const markupIn = (code) =>
  MARKUP.filter(([pattern]) => pattern.test(code)).map(([, name]) => name);

test("the window's scripts never turn a string into markup or code", () => {
  const names = scripts().map((script) => script.name);
  assert.ok(names.length >= 14, `${names.length} scripts were read`);
  for (const name of names) {
    assert.deepEqual(markupIn(source(name)), [], name);
  }
  const bad = {
    innerHTML: "el.innerHTML = name;",
    outerHTML: "el.outerHTML = name;",
    insertAdjacentHTML: "el.insertAdjacentHTML('beforeend', name);",
    "document.write": "document.write(name);",
    eval: "eval(name);",
    Function: "new Function(name)();",
    "a timer given a string": "setTimeout('x()', 1);",
    "an HTML or code parser": "new DOMParser().parseFromString(name, 'text/html');",
    "a dynamic import": "import(name);",
  };
  for (const [name, code] of Object.entries(bad)) {
    assert.ok(markupIn(code).includes(name), `the scan misses ${name}`);
  }
});

test("the window's page loads nothing, runs nothing of its own and has no handler written in its markup", () => {
  const page = source("confirm.html");
  const csp = page.match(/http-equiv="Content-Security-Policy"\s+content="([^"]+)"/)?.[1];
  assert.equal(csp, "default-src 'none'; script-src 'self'; style-src 'self'");
  const scripts = [...page.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
  assert.equal(scripts.length, 1);
  assert.match(scripts[0][1], /\bsrc="confirm\.js"/);
  assert.equal(scripts[0][2].trim(), "", "an inline script");
  assert.deepEqual(page.match(/\son[a-z]+\s*=/gi) ?? [], [], "an event handler attribute");
  assert.deepEqual(page.match(/\sstyle\s*=/gi) ?? [], [], "an inline style");
  assert.deepEqual(page.match(/<style\b/gi) ?? [], [], "an inline style element");
  assert.deepEqual(
    page.match(/\b(?:src|href|action|data|poster|srcset)\s*=\s*"[^"]*"/gi).sort(),
    ['href="confirm.css"', 'src="confirm.js"'],
  );
  assert.deepEqual(page.match(/<(iframe|frame|object|embed|img|link|form|base|meta)\b/gi)?.sort(), [
    "<link",
    "<meta",
    "<meta",
  ]);
  assert.deepEqual(source("confirm.css").match(/url\s*\(|@import|@font-face/gi) ?? [], []);
});

// Brave 1.98.62 gives the window 568 pixels of height, and the page of the
// longest drag, typed text or name is 820 to 970 pixels tall, which would put the buttons at the end
// of the page out of view. The bar stays at the foot of the window and the rest of the
// page scrolls under it.
test("the wait line, the status and the buttons are one bar that stays at the foot of the window while the rest of the page scrolls", () => {
  const page = source("confirm.html");
  const opening = page.indexOf('<div class="bar">');
  assert.ok(opening > 0, "the page has no bar");
  // The bar is the div that opens there and the div that closes it, with the
  // div of the buttons inside.
  const divs = /<(\/?)div\b[^>]*>/g;
  divs.lastIndex = opening;
  let depth = 0;
  let closing = -1;
  for (let tag = divs.exec(page); tag !== null && closing < 0; tag = divs.exec(page)) {
    depth += tag[1] === "/" ? -1 : 1;
    closing = depth === 0 ? divs.lastIndex : -1;
  }
  assert.ok(closing > opening, "the bar is not closed");
  const before = page.slice(0, opening);
  const bar = page.slice(opening, closing);
  assert.equal(page.slice(closing, page.indexOf("<script")).trim(), "", "something follows the bar");
  for (const id of ["wait", "status", "deny", "allow"]) {
    assert.equal(before.includes(`id="${id}"`), false, `${id} is above the bar`);
    assert.equal(bar.includes(`id="${id}"`), true, `${id} is not in the bar`);
  }
  assert.deepEqual(bar.match(/<(?:h1|h2|dl|pre|section|label|input)\b/g) ?? [], [], "the bar holds the page's words");
  const rule = source("confirm.css").match(/(?:^|\n)\.bar\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(rule, /position:\s*sticky/);
  assert.match(rule, /bottom:\s*0\s*;/);
  assert.match(rule, /background:\s*(?!transparent|none)\S+/, "text would show through the bar");
});

// An action with no element hides the block of the element, so the block is one
// section: the heading, every row of the element and the note about the outline,
// and nothing that every action shows.
test("the element block of the window is one section that holds the heading, every element row and the note about the outline, and none of the page, the typed text or the bar", () => {
  const page = source("confirm.html");
  const opening = page.indexOf('<section id="element-section">');
  assert.ok(opening > 0, "the page has no element section");
  const closing = page.indexOf("</section>", opening);
  assert.ok(closing > opening, "the element section is not closed");
  const block = page.slice(opening, closing);
  assert.match(block, /<h2>The element, in the page's own words<\/h2>/);
  for (const id of ["number-term", "number", "role", "tag", "name", "excerpt", "goes"]) {
    assert.equal(block.includes(`id="${id}"`), true, `${id} is outside the element section`);
  }
  assert.match(block, /An outline around the element in the page is a hint/);
  for (const id of ["action", "typed", "typed-heading", "typed-count", "origin", "url", "wait", "status", "allow", "deny"]) {
    assert.equal(block.includes(`id="${id}"`), false, `${id} is inside the element section`);
  }
  // Nothing of the element is left outside it.
  const rest = page.slice(0, opening) + page.slice(closing);
  for (const id of ["number-term", "number", "role", "tag", "name", "excerpt", "goes"]) {
    assert.equal(rest.includes(`id="${id}"`), false, `${id} is also outside the element section`);
  }
  assert.equal(rest.includes("An outline around the element"), false, "the note is also outside the element section");
  assert.equal((page.match(/<section\b/g) ?? []).length, 2, "the typed text and the element are the two sections");
});

test("no web page can open, frame or send a message to the windows or the worker, and no extension but the BraveBot extension can send the worker a message", () => {
  const manifest = JSON.parse(source("manifest.json"));
  assert.equal(manifest.web_accessible_resources, undefined);
  assert.equal(manifest.content_scripts, undefined);
  assert.deepEqual(manifest.externally_connectable, { ids: [FIRST_ID] });
  assert.ok(!JSON.stringify(manifest).includes("confirm"));
  assert.ok(!JSON.stringify(manifest).includes("dialog"));
  assert.equal(CONFIRMATION_PORT, "bravebot-confirmation");
});

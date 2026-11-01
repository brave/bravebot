// The dialog panel: the page the person answers a dialog in (the real
// dialog.js) and the service worker that opens it (windows.js over the shared
// confirmation.js), over the fake browser and pages of fake-panels.mjs. The tests
// act as the person, pressing the buttons and typing in the field, and as a page
// that wrote the dialog's words.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, mock, test } from "node:test";
import { ALLOW_DELAY_MS, CONFIRMATION_PORT, HEARTBEAT_MS } from "../approval.js";
import { spend } from "../approval.js";
import { createWindows } from "../windows.js";
import { TARGET_LIMIT, TYPED_TEXT_LIMIT, URL_LIMIT } from "../words.js";
import { browser, flush } from "./fake-panels.mjs";

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] }));
afterEach(() => mock.timers.reset());

const tick = async (ms) => {
  mock.timers.tick(ms);
  await flush();
};

const WHERE = { origin: "https://shop.example", url: "https://shop.example/cart" };
const NAME = JSON.parse(readFileSync(new URL("../manifest.json", import.meta.url), "utf8")).name;

// A panel asked about `dialog`: an action was asked about first, as a call's
// dialog always follows one, and its window is left open.
async function panelFor(dialog = {}, where = WHERE, options) {
  const fake = browser({ ...options, page: { name: NAME, ...options?.page } });
  const windows = createWindows(fake.chrome);
  windows.listen();
  const first = Object.freeze({ description: { action: "click" } });
  windows.ask(first).catch(() => {});
  await flush();
  const state = { done: false, value: undefined, error: undefined };
  state.promise = windows
    .askDialog({ type: "alert", message: "Hello", defaultPrompt: "", ...dialog }, { ...where, first })
    .then(
      (value) => {
        state.done = true;
        state.value = value;
      },
      (error) => {
        state.done = true;
        state.error = error;
      },
    );
  await flush();
  const page = fake.pages.findLast((each) => each.file === "dialog.html");
  return { fake, windows, page, question: state };
}

const text = (page, id) => page.element(id).shown();
const plain = (page, id) => page.element(id).textContent;

test("the panel names the kind of dialog in its own words and shows the site, the address and what the dialog says as text, and a field for the answer to a prompt only", async () => {
  const kinds = {
    alert: ["The page shows a message and waits for OK", true],
    confirm: ["The page asks a question that is answered yes or no", true],
    prompt: ["The page asks a question that is answered with text", false],
    beforeunload: ["The page asks whether to leave it", true],
  };
  for (const [type, [words, hidden]] of Object.entries(kinds)) {
    const { page } = await panelFor({ type, message: "Delete everything?", defaultPrompt: "Ada" });
    assert.equal(plain(page, "kind"), words, type);
    assert.equal(page.element("prompt-section").hidden, hidden, type);
    assert.equal(plain(page, "origin"), "https://shop.example");
    assert.equal(plain(page, "url"), "https://shop.example/cart");
    assert.deepEqual(text(page, "message"), ["Delete everything?"]);
    if (type === "prompt") {
      assert.deepEqual(text(page, "suggested"), ["Ada"]);
      assert.equal(page.element("answer").value, "", "the field starts with the page's suggestion");
    }
  }
  const { page } = await panelFor({ type: "alert", message: "" });
  assert.deepEqual(text(page, "message"), [{ mark: "nothing" }]);
});

test("the heading and the title of the panel name this extension, as its manifest does", async () => {
  const { page } = await panelFor();
  assert.equal(plain(page, "heading"), `How should ${NAME} answer this dialog?`);
  assert.equal(page.document.title, `${NAME} asks how to answer a dialog`);
});

// What a page is able to put in a dialog: markup, a line break, an override that
// draws the end of a word first, a character with no width, a space that is not
// one.
test("markup in a dialog's message and suggested answer is text, and control, direction and invisible characters are marked with their code points", async () => {
  const markup = `<img src=x onerror=alert(1)><script>alert(2)</script>`;
  const { page } = await panelFor(
    {
      type: "prompt",
      message: `${markup}\nsecond\u202eevil\u0007 \u200b\u00a0end`,
      defaultPrompt: "ok\u202egpj.exe\u0000",
    },
    { origin: "https://shop.example", url: "https://shop.example/\u2029x" },
  );
  assert.deepEqual(text(page, "message"), [
    markup,
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
  assert.deepEqual(text(page, "suggested"), ["ok", { mark: "U+202E" }, "gpj.exe", { mark: "U+0000" }]);
  assert.deepEqual(text(page, "url"), ["https://shop.example/", { mark: "U+2029" }, "x"]);
});

test("the page's words are cut to their limits and the panel says how much was left out", async () => {
  const long = (limit) => "w".repeat(limit + 50);
  const { page } = await panelFor(
    { type: "prompt", message: long(TYPED_TEXT_LIMIT), defaultPrompt: long(TARGET_LIMIT) },
    { origin: long(URL_LIMIT), url: long(URL_LIMIT) },
  );
  for (const [id, limit] of [
    ["message", TYPED_TEXT_LIMIT],
    ["suggested", TARGET_LIMIT],
    ["origin", URL_LIMIT],
    ["url", URL_LIMIT],
  ]) {
    assert.deepEqual(text(page, id), ["w".repeat(limit), { mark: "50 more characters not shown" }], id);
  }
  await tick(ALLOW_DELAY_MS);
  assert.equal(page.element("accept").disabled, false, "a long message cannot be answered");
});

test("Dismiss has the focus, and Accept is not available until a second after the panel is shown, nor while the panel is out of front", async () => {
  const { page, question } = await panelFor();
  assert.equal(page.focused, page.element("dismiss"));
  assert.equal(page.element("accept").disabled, true);
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(page.element("accept").disabled, true);
  await page.click("accept");
  assert.equal(question.done, false, "a click at once answered");
  await tick(1);
  assert.equal(page.element("accept").disabled, false);
  assert.equal(page.focused, page.element("dismiss"), "the focus moved to Accept");
  page.blur();
  assert.equal(page.element("accept").disabled, true, "Accept stayed available out of front");
  await page.click("accept");
  assert.equal(question.done, false);
  page.focus();
  await tick(ALLOW_DELAY_MS - 1);
  assert.equal(page.element("accept").disabled, true, "Accept came back at once");
  await tick(1);
  await page.click("accept");
  await question.promise;
  assert.equal(question.value.accept, true);

  const hidden = await panelFor({}, WHERE, { page: { focused: false } });
  await tick(5 * ALLOW_DELAY_MS);
  assert.equal(hidden.page.element("accept").disabled, true, "a panel never in front was answered");
});

test("a click that is not the person's, or that no user gesture made, does not accept", async () => {
  const { page, question } = await panelFor();
  await tick(ALLOW_DELAY_MS);
  await page.click("accept", { isTrusted: false });
  assert.equal(question.done, false, "a script's click accepted");
  page.active = false;
  await page.click("accept");
  assert.equal(question.done, false, "a click with no user activation accepted");
  page.active = true;
  await page.click("accept");
  await question.promise;
  assert.equal(question.value.accept, true);
});

test("Accept and Dismiss both answer the dialog, each with a token for it: Accept with the text typed for a prompt, Dismiss and Escape with none", async () => {
  const ways = {
    "Accept a prompt": [{ type: "prompt" }, "accept", "Grace", { accept: true, text: "Grace" }],
    "Accept a prompt with nothing typed": [{ type: "prompt" }, "accept", "", { accept: true, text: "" }],
    "Accept a confirm": [{ type: "confirm" }, "accept", "stray", { accept: true, text: "" }],
    "Dismiss a prompt": [{ type: "prompt" }, "dismiss", "Grace", { accept: false, text: "" }],
    "Dismiss a confirm": [{ type: "confirm" }, "dismiss", "", { accept: false, text: "" }],
    "Escape": [{ type: "prompt" }, "Escape", "Grace", { accept: false, text: "" }],
  };
  for (const [way, [dialog, control, typed, expected]] of Object.entries(ways)) {
    const { fake, page, question } = await panelFor(dialog);
    await tick(ALLOW_DELAY_MS);
    page.type("answer", typed);
    if (control === "Escape") {
      await page.key("Escape");
    } else {
      await page.click(control);
    }
    await question.promise;
    assert.equal(question.error, undefined, way);
    assert.equal(question.value.accept, expected.accept, way);
    assert.equal(question.value.text, expected.text, way);
    assert.equal(spend(question.value.token, question.value.resolution), true, way);
    const sent = page.ports.page.sent.filter((message) => message.type === "answer");
    assert.deepEqual(sent, [{ type: "answer", allow: true, ...expected, accept: expected.accept }], way);
    assert.equal(page.element("accept").disabled, true);
    assert.equal(page.element("dismiss").disabled, true);
    assert.match(plain(page, "status"), expected.accept ? /^Accepted/ : /^Dismissed/, way);
    // Answered once: a second press sends nothing more.
    await page.click("dismiss");
    await page.key("Escape");
    assert.equal(page.ports.page.sent.filter((message) => message.type === "answer").length, 1, way);
    assert.deepEqual(fake.windowsOpen().length, 1, "only the action window is left");
  }
});

test("a dialog the panel cannot show whole is not answered from it, and stays open", async () => {
  const bad = {
    "a kind of dialog it does not know": { type: "notify" },
    "a message that is not text": { message: 5 },
    "a suggested answer that is not text": { type: "prompt", defaultPrompt: undefined },
  };
  for (const [why, dialog] of Object.entries(bad)) {
    const { page, question } = await panelFor(dialog);
    await tick(5 * ALLOW_DELAY_MS);
    assert.equal(page.element("accept").disabled, true, why);
    await page.click("accept");
    await page.key("Escape");
    assert.match(plain(page, "status"), /cannot be shown, so it cannot be answered here/, why);
    assert.equal(page.ports.page.sent.some((message) => message.type === "answer"), false, why);
    assert.equal(question.done, false, why);
    await tick(60_000);
    assert.equal(question.error?.code, "dialog_open", why);
  }
});

test("the panel counts down, tells the worker it is still there while it waits, and stops when answered", async () => {
  const { page, question } = await panelFor();
  const alive = () => page.ports.page.sent.filter((message) => message.type === "alive").length;
  assert.equal(plain(page, "wait"), "The dialog stays open in the page if you do not answer in 60 seconds.");
  await tick(HEARTBEAT_MS - 1000);
  assert.equal(alive(), 0);
  await tick(1000);
  assert.equal(alive(), 1);
  assert.equal(plain(page, "wait"), "The dialog stays open in the page if you do not answer in 45 seconds.");
  await tick(ALLOW_DELAY_MS);
  await page.click("dismiss");
  await question.promise;
  const counted = plain(page, "wait");
  await tick(3 * HEARTBEAT_MS);
  assert.equal(alive(), 1, "the panel kept sending after it was answered");
  assert.equal(plain(page, "wait"), counted, "the panel went on counting after it was answered");
});

test("a panel whose worker stops, or that is told its request ended, says the dialog stays open, answers nothing and closes", async () => {
  const { page, question } = await panelFor();
  await tick(ALLOW_DELAY_MS);
  const sent = page.ports.page.sent.length;
  page.ports.worker.disconnect();
  await flush();
  assert.equal(page.closed, true, "the panel stayed open");
  assert.equal(page.element("accept").disabled, true);
  assert.equal(page.element("dismiss").disabled, true);
  assert.match(plain(page, "status"), /no longer waiting for this window, so the dialog stays open in the page/);
  await page.click("accept");
  await page.key("Escape");
  assert.equal(page.ports.page.sent.length, sent, "the panel sent something after the port closed");
  await tick(60_000);
  await question.promise;
  assert.equal(question.error.code, "dialog_open");

  const stray = browser({ opening: false });
  const windows = createWindows(stray.chrome);
  windows.listen();
  const lone = await stray.open({ file: "dialog.html", window: { id: 3, tabs: [{ id: 30 }] } });
  assert.equal(lone.closed, true, "a panel with no question stayed open");
  assert.match(plain(lone, "status"), /This request has ended|no longer waiting for this window/);
});

const source = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");

test("the panel's page loads nothing, runs nothing of its own and has no handler written in its markup, and its one field takes the answer to a prompt", () => {
  const page = source("dialog.html");
  const csp = page.match(/http-equiv="Content-Security-Policy"\s+content="([^"]+)"/)?.[1];
  assert.equal(csp, "default-src 'none'; script-src 'self'; style-src 'self'");
  const scripts = [...page.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
  assert.equal(scripts.length, 1);
  assert.match(scripts[0][1], /\bsrc="dialog\.js"/);
  assert.equal(scripts[0][2].trim(), "", "an inline script");
  assert.deepEqual(page.match(/\son[a-z]+\s*=/gi) ?? [], [], "an event handler attribute");
  assert.deepEqual(page.match(/\sstyle\s*=/gi) ?? [], [], "an inline style");
  assert.deepEqual(page.match(/<style\b/gi) ?? [], [], "an inline style element");
  assert.deepEqual(
    page.match(/\b(?:src|href|action|data|poster|srcset)\s*=\s*"[^"]*"/gi).sort(),
    ['href="confirm.css"', 'href="dialog.css"', 'src="dialog.js"'],
  );
  assert.deepEqual(page.match(/<(iframe|frame|object|embed|img|link|form|base|meta|input|textarea)\b/gi)?.sort(), [
    "<input",
    "<link",
    "<link",
    "<meta",
    "<meta",
  ]);
  assert.match(page, /<input id="answer" type="text" maxlength="2000"/);
  assert.equal(TYPED_TEXT_LIMIT, 2000);
  assert.deepEqual(source("dialog.css").match(/url\s*\(|@import|@font-face/gi) ?? [], []);
  assert.equal(CONFIRMATION_PORT, "bravebot-confirmation");
});

// Brave 1.98.62 gives the window 568 pixels of height, and the page of the
// longest dialog message is 820 to 970 pixels tall, which would put the buttons at the end of
// the page out of view. The bar stays at the foot of the window and the rest of the
// page scrolls under it.
test("the wait line, the status and the buttons are one bar that stays at the foot of the window while the rest of the page scrolls", () => {
  const page = source("dialog.html");
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
  for (const id of ["wait", "status", "dismiss", "accept"]) {
    assert.equal(before.includes(`id="${id}"`), false, `${id} is above the bar`);
    assert.equal(bar.includes(`id="${id}"`), true, `${id} is not in the bar`);
  }
  assert.deepEqual(bar.match(/<(?:h1|h2|dl|pre|section|label|input)\b/g) ?? [], [], "the bar holds the page's words");
  const rule = source("confirm.css").match(/(?:^|\n)\.bar\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(rule, /position:\s*sticky/);
  assert.match(rule, /bottom:\s*0\s*;/);
  assert.match(rule, /background:\s*(?!transparent|none)\S+/, "text would show through the bar");
});

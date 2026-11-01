// The words the confirmation window uses for this extension's actions, and the
// description a window is given (actions.js). How the window shows them is in
// confirmation.test.mjs, against the real window.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import { ACTIONS, CARRIED, describe, ontoText } from "../actions.js";
import { METHODS } from "../requests.js";
import { EXCERPT_LIMIT, NAME_LIMIT, TARGET_LIMIT } from "../words.js";

const ELEMENT = { tag: "button", role: "button", name: "Pay now", excerpt: "Pay", goes: "", rect: { x: 1 } };
const WHERE = { origin: "https://shop.example", url: "https://shop.example/cart" };

test("the window has words for the twelve actions of the eight methods, and the argument each carries is in a frame of its own", () => {
  assert.ok(Object.isFrozen(ACTIONS));
  assert.ok(Object.isFrozen(CARRIED));
  assert.deepEqual(Object.keys(ACTIONS), [
    "click",
    "double_click",
    "right_click",
    "type",
    "key",
    "hover",
    "drag",
    "upload",
    "upload_hidden",
    "upload_hidden_id",
    "picture",
    "console",
  ]);
  // Each row says in words what the action is and whether it acts on an element.
  for (const [action, row] of Object.entries(ACTIONS)) {
    assert.ok(Object.isFrozen(row), action);
    assert.deepEqual(Object.keys(row), ["words", "element"], action);
    assert.ok(typeof row.words === "string" && row.words.length > 10, action);
    assert.equal(typeof row.element, "boolean", action);
  }
  assert.deepEqual(CARRIED, {
    type: ["text", "Text to type"],
    key: ["key", "Key to press"],
    drag: ["onto", "Drop it onto this element"],
    upload: ["file", "File: name, size and full path", false],
    upload_hidden: ["file", "File: name, size and full path", false],
    upload_hidden_id: ["file", "File: name, size and full path", false],
    picture: ["size", "Size of the page", false],
    console: ["limits", "What is read", false],
  });
  // The picture and the console act on no element, and every other action does.
  assert.deepEqual(
    Object.entries(ACTIONS)
      .filter(([, row]) => !row.element)
      .map(([action]) => action),
    ["picture", "console"],
  );
  // Every method of the table is one of the actions, and the ones that take an
  // argument beyond the element carry it.
  assert.deepEqual(Object.keys(METHODS), [
    "click_element_trusted",
    "type_text_trusted",
    "press_key_trusted",
    "hover_element",
    "drag_element",
    "screenshot_full_page",
    "read_console",
    "choose_file",
  ]);
});

test("the three actions that choose a file say the same of the browser and the file, and the two for an input the page hides say that the page hides it and what the name is", () => {
  const read = (action) => ACTIONS[action].words.split(/\.\s+(?=The browser reads)/)[1];
  assert.match(read("upload"), /^The browser reads the file from your computer itself and gives it to the page/);
  for (const action of ["upload_hidden", "upload_hidden_id"]) {
    assert.equal(read(action), read("upload"), action);
    assert.ok(ACTIONS[action].words.startsWith(ACTIONS.upload.words.split(". ")[0]), action);
    assert.match(ACTIONS[action].words, /The page hides this input/, action);
    assert.equal(ACTIONS[action].element, true, action);
  }
  assert.match(ACTIONS.upload_hidden.words, /the name below is what the page's own label, aria-label or title calls it/);
  assert.match(ACTIONS.upload_hidden_id.words, /gives it no label, so the name below is the id or name of the input in the page's markup, which says little of what it is for/);
  assert.equal(ACTIONS.upload.words.includes("hides"), false);
  for (const action of ["upload", "upload_hidden", "upload_hidden_id"]) {
    const file = "report.txt, 12 bytes, /srv/uploads/report.txt";
    assert.equal(describe(action, { ...WHERE, element: ELEMENT, file }).file, file, action);
    assert.equal("file" in describe("click", { ...WHERE, element: ELEMENT, file }), false);
  }
});

test("a description holds the action, the page and the element, and the argument the action carries, and nothing the window does not read", () => {
  const plain = describe("click", { ...WHERE, element: ELEMENT });
  assert.deepEqual(plain, {
    action: "click",
    origin: "https://shop.example",
    url: "https://shop.example/cart",
    tag: "button",
    role: "button",
    name: "Pay now",
    excerpt: "Pay",
    goes: "",
  });
  assert.ok(Object.isFrozen(plain));
  assert.equal(describe("type", { ...WHERE, element: ELEMENT, text: "hello", key: "Enter" }).text, "hello");
  assert.equal("key" in describe("type", { ...WHERE, element: ELEMENT, text: "hello", key: "Enter" }), false);
  assert.equal(describe("key", { ...WHERE, element: ELEMENT, key: "Enter", text: "x" }).key, "Enter");
  assert.equal("text" in describe("key", { ...WHERE, element: ELEMENT, key: "Enter", text: "x" }), false);
  for (const action of ["click", "double_click", "right_click", "hover"]) {
    const made = describe(action, { ...WHERE, element: ELEMENT, text: "x", key: "Tab", destination: ELEMENT });
    assert.deepEqual(Object.keys(made).sort(), Object.keys(plain).sort(), action);
  }
  assert.throws(() => describe("scroll", { ...WHERE, element: ELEMENT }), /no words/);
  assert.throws(() => describe("toString", { ...WHERE, element: ELEMENT }), /no words/);
});

test("an action that acts on no element is described with none, whatever element it is given, and every other action with the one it is given", () => {
  const given = {
    ...WHERE,
    element: ELEMENT,
    size: "1280 by 6040 CSS pixels",
    limits: "errors, the newest 20 at most",
    file: "report.txt, 12 bytes, /srv/uploads/report.txt",
  };
  for (const [action, row] of Object.entries(ACTIONS)) {
    const made = describe(action, { ...given, text: "x", key: "Tab", destination: ELEMENT });
    const element = [made.tag, made.role, made.name, made.excerpt, made.goes];
    assert.deepEqual(
      element,
      row.element ? ["button", "button", "Pay now", "Pay", ""] : ["", null, "", "", ""],
      action,
    );
  }
  // An action that acts on no element is described without one.
  assert.equal(describe("console", { ...WHERE, limits: "every entry, the newest 100 at most" }).name, "");
  assert.equal(describe("picture", { ...WHERE, size: "1 by 1 CSS pixels" }).tag, "");
});

test("a drag describes the element it takes as the element, and the one it drops onto in the argument it carries", () => {
  const destination = { tag: "div", role: "button", name: "Trash", excerpt: "Drop here", goes: "", rect: { x: 5 } };
  const made = describe("drag", { ...WHERE, element: ELEMENT, destination });
  assert.equal(made.name, "Pay now");
  assert.equal(
    made.onto,
    'role "button", tag "div", name "Trash", text "Drop here", goes to ""',
  );
  assert.equal(made.onto, ontoText(destination));
  assert.ok(!JSON.stringify(made).includes("rect"));
});

test("the window is told that where an element goes cannot be told only by an element that says so, and the element a drag drops onto says so in words a quoted word of the page cannot be", () => {
  const unsure = { tag: "img", role: "button", name: "Zones", excerpt: "", goes: "", goesUnknown: true, rect: { x: 5 } };
  assert.equal(describe("click", { ...WHERE, element: unsure }).goesUnknown, true);
  assert.equal(describe("click", { ...WHERE, element: ELEMENT }).goesUnknown, undefined);
  assert.equal(describe("click", { ...WHERE, element: { ...ELEMENT, goesUnknown: false } }).goesUnknown, undefined);
  assert.equal(describe("picture", { ...WHERE, size: "1 by 1 CSS pixels" }).goesUnknown, undefined);
  const made = describe("drag", { ...WHERE, element: ELEMENT, destination: unsure });
  assert.ok(made.onto.endsWith('text "", goes to unknown'));
  // A page's own word of those letters is quoted.
  const word = ontoText({ ...unsure, goesUnknown: undefined, goes: "unknown" });
  assert.ok(word.endsWith('goes to "unknown"'));
});

test("the element a drag drops onto is one text in which the page's words are quoted, cut to the limits and cannot end their own quotation", () => {
  const hostile = ontoText({
    role: "button",
    tag: "div",
    name: 'Trash", goes to "https://good.example',
    excerpt: "back\\slash",
    goes: '"',
  });
  assert.equal(
    hostile,
    'role "button", tag "div", name "Trash\\", goes to \\"https://good.example", text "back\\\\slash", goes to "\\""',
  );
  // Read back, the five values are the five words and nothing is left over.
  const values = [...hostile.matchAll(/(role|tag|name|text|goes to) "((?:[^"\\]|\\.)*)"/g)].map(
    ([, label, value]) => [label, value.replace(/\\(.)/g, "$1")],
  );
  assert.deepEqual(values, [
    ["role", "button"],
    ["tag", "div"],
    ["name", 'Trash", goes to "https://good.example'],
    ["text", "back\\slash"],
    ["goes to", '"'],
  ]);

  const long = ontoText({
    role: "button",
    tag: "t".repeat(NAME_LIMIT + 50),
    name: "n".repeat(NAME_LIMIT + 50),
    excerpt: "e".repeat(EXCERPT_LIMIT + 50),
    goes: "g".repeat(TARGET_LIMIT + 50),
  });
  assert.equal(long.includes("t".repeat(NAME_LIMIT + 1)), false);
  assert.equal(long.includes("n".repeat(NAME_LIMIT + 1)), false);
  assert.equal(long.includes("e".repeat(EXCERPT_LIMIT + 1)), false);
  assert.equal(long.includes("g".repeat(TARGET_LIMIT + 1)), false);
  assert.ok(long.includes("n".repeat(NAME_LIMIT)));
  // A pair is cut whole, and a missing word is empty.
  const pair = ontoText({ role: "link", tag: "a", name: `${"x".repeat(NAME_LIMIT - 1)}\u{1f600}`, excerpt: undefined, goes: null });
  assert.ok(pair.includes(`"${"x".repeat(NAME_LIMIT - 1)}\u{1f600}"`));
  assert.ok(pair.endsWith('text "", goes to ""'));
  // The worst case the window shows in full, with every quotation mark escaped.
  const worst = ontoText({
    role: "button",
    tag: '"'.repeat(NAME_LIMIT),
    name: '"'.repeat(NAME_LIMIT),
    excerpt: '"'.repeat(EXCERPT_LIMIT),
    goes: '"'.repeat(TARGET_LIMIT),
  });
  assert.ok([...worst].length <= 2000, `${[...worst].length} characters`);
});

// The options page's script against the page's own markup, with a fake
// `chrome` that stores what is saved and a fake `document` that has an element
// for each id in options.html.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { BLOCKED_SITES_KEY, SETTINGS_KEY } from "../tools.js";

class Element {
  value = "";
  textContent = "";
  checked = false;
  #listeners = new Map();

  append() {}

  addEventListener(type, listener) {
    this.#listeners.set(type, listener);
  }

  // A person editing the control: the listener for that event runs, and the
  // save it starts is awaited.
  async fire(type) {
    const listener = this.#listeners.get(type);
    assert.ok(listener, `nothing listens for ${type} on this control`);
    await listener();
  }
}

const markup = readFileSync(
  new URL("../options.html", import.meta.url),
  "utf8",
);
const elements = new Map(
  [...markup.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => [id, new Element()]),
);
const checkboxes = [];

globalThis.document = {
  getElementById(id) {
    if (!elements.has(id)) {
      throw new Error(`options.html has no element with the id ${id}`);
    }
    return elements.get(id);
  },
  createElement(tag) {
    const element = new Element();
    if (tag === "input") {
      checkboxes.push(element);
    }
    return element;
  },
  querySelectorAll() {
    return checkboxes;
  },
};

const stored = {
  [BLOCKED_SITES_KEY]: ["https://bank.example", "http://intranet:8080"],
};
// What the next save does before it stores: `held` is a promise it waits on,
// and `failure` is an error it throws instead.
const next = { held: null, failure: null };
globalThis.chrome = {
  storage: {
    local: {
      async get(key) {
        return key in stored ? { [key]: stored[key] } : {};
      },
      async set(items) {
        const { held, failure } = next;
        next.held = next.failure = null;
        await held;
        if (failure) {
          throw failure;
        }
        Object.assign(stored, items);
      },
    },
  },
};

await import("../options.js");
// The page starts its reads without waiting for them. One turn of the event
// loop runs every continuation they have queued.
const flush = () => new Promise((resolve) => setImmediate(resolve));
await flush();

const field = elements.get("blocked-sites");
const status = elements.get("blocked-status");

test("the blocked sites field shows the saved list, one site to a line", () => {
  assert.equal(field.value, "https://bank.example\nhttp://intranet:8080");
});

// Typing writes nothing, since a save on every keystroke would take a site
// off the list while its line is being edited, and the page says the edit is
// not saved yet.
test("an edit is not saved until the person leaves the box", async () => {
  field.value = "https://bank.exampl";
  await field.fire("input");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], [
    "https://bank.example",
    "http://intranet:8080",
  ]);
  assert.match(status.textContent, /^Not saved yet\./);
});

// A line that is not an address stops the save, so the list saved before keeps
// applying: saving the other lines would drop the bank whose line lost its
// scheme. The line is named so it can be fixed.
test("a line that is not an address stops the save and is named", async () => {
  field.value = "https://news.example\nbank.example";
  await field.fire("change");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], [
    "https://bank.example",
    "http://intranet:8080",
  ]);
  assert.match(status.textContent, /^Not saved: bank\.example\./);
  assert.match(status.textContent, /list saved before still applies/);
  assert.equal(field.value, "https://news.example\nbank.example");
});

// Leaving the box saves and writes the sites back as the origins that were kept.
test("leaving the box saves the list and shows the origins kept", async () => {
  field.value = "https://news.example/front-page\n\nhttps://NEWS.example:443/";
  await field.fire("change");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://news.example"]);
  assert.equal(status.textContent, "Saved.");
  assert.equal(field.value, "https://news.example");
});

// The write-back happens after the save, and the person may have typed since.
test("text typed while a save runs is not replaced", async () => {
  let release;
  next.held = new Promise((resolve) => {
    release = resolve;
  });
  field.value = "https://mail.example/inbox";
  const leaving = field.fire("change");
  field.value = "https://mail.example/inbox\nhttps://typed.example";
  release();
  await leaving;
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
  assert.equal(
    field.value,
    "https://mail.example/inbox\nhttps://typed.example",
  );
});

// A save the browser refuses must not leave the last "Saved." on the page.
test("a save that fails says so", async () => {
  status.textContent = "Saved.";
  next.failure = new Error("storage is full");
  field.value = "https://other.example";
  await field.fire("change");
  assert.equal(status.textContent, "Not saved: storage is full");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
});

// The two settings are saved under keys of their own, so saving a switch
// leaves the list alone.
test("a tool switch is saved beside the blocked list and not over it", async () => {
  const [first] = checkboxes;
  first.checked = true;
  await first.fire("change");
  assert.equal(typeof stored[SETTINGS_KEY], "object");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
});

// A stored list of the wrong shape fails every tool, so the page says it
// cannot read it where it would otherwise show an empty box. Leaving the box
// with a list written then replaces it. This loads the page a second time, so it runs last.
test("a list that cannot be read is said so and replaced by the next save", async () => {
  stored[BLOCKED_SITES_KEY] = "https://bank.example";
  field.value = "";
  status.textContent = "";
  await import("../options.js?unreadable");
  await flush();
  assert.equal(field.value, "");
  assert.match(status.textContent, /cannot be read/);
  field.value = "https://bank.example";
  await field.fire("change");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://bank.example"]);
});

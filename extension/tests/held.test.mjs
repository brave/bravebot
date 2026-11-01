// What a person holds out in the browser: the items the extension's context
// menu entries make, how many are kept and for how long, how they are taken and
// forgotten, and the entries themselves. What take_held returns from them, and
// what turning the group off does to them, is tested with the tools and the
// switches.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  HELD_KEY,
  MAX_ADDRESS,
  MAX_AGE_MS,
  MAX_ITEMS,
  MAX_TEXT,
  MENU,
  forgetHeld,
  heldCount,
  holdItem,
  itemFrom,
  listenForHolds,
  showMenus,
  takeHeld,
} from "../held.js";
import { fakeChrome } from "./fake-chrome.mjs";
import { withMenus } from "./fake-menus.mjs";

const NOW = 1_700_000_000_000;
const PAGE = "https://news.example/story?id=7";

const SELECTION = {
  menuItemId: "bravebot-hold-selection",
  selectionText: "the part I selected",
  pageUrl: PAGE,
};
const LINK = {
  menuItemId: "bravebot-hold-link",
  linkUrl: "https://shop.example/cart",
  pageUrl: PAGE,
};
const TAB = { menuItemId: "bravebot-hold-tab", pageUrl: PAGE };

const flush = () => new Promise((resolve) => setImmediate(resolve));

const kept = (chrome) => chrome.session[HELD_KEY];

test("the entries are one for a selection, one for a link and one for a tab, each in the context it names", () => {
  assert.deepEqual(
    MENU.map(({ id, kind, contexts }) => [id, kind, contexts]),
    [
      ["bravebot-hold-selection", "selection", ["selection"]],
      ["bravebot-hold-link", "link", ["link"]],
      ["bravebot-hold-tab", "tab", ["page", "frame"]],
    ],
  );
  for (const { title } of MENU) {
    assert.match(title, /^Hold this (selection|link|tab) for BraveBot$/);
  }
});

test("a use of an entry makes the item the entry is for, from what the browser gave and nothing else", () => {
  const origin = "https://news.example";
  assert.deepEqual(itemFrom(SELECTION, NOW), {
    kind: "selection",
    time: NOW,
    text: "the part I selected",
    page: PAGE,
    origin,
  });
  assert.deepEqual(itemFrom(LINK, NOW), {
    kind: "link",
    time: NOW,
    address: "https://shop.example/cart",
    page: PAGE,
    origin,
  });
  assert.deepEqual(itemFrom(TAB, NOW), { kind: "tab", time: NOW, page: PAGE, origin });
  // A field the entry is not for is not read: a tab entry used where the
  // browser also reports a selection holds no text.
  assert.deepEqual(
    itemFrom({ ...TAB, selectionText: "text", linkUrl: "https://a.example/" }, NOW),
    { kind: "tab", time: NOW, page: PAGE, origin },
  );
});

test("an entry that is not ours, and one the browser gave nothing to hold for, make no item", () => {
  for (const info of [
    undefined,
    {},
    { ...SELECTION, menuItemId: "another-extensions-entry" },
    { ...SELECTION, selectionText: "" },
    { ...SELECTION, selectionText: undefined },
    { ...SELECTION, selectionText: 7 },
    { ...LINK, linkUrl: "" },
    { menuItemId: LINK.menuItemId, pageUrl: PAGE },
    { ...TAB, pageUrl: "" },
    { menuItemId: TAB.menuItemId },
  ]) {
    assert.equal(itemFrom(info, NOW), null, JSON.stringify(info));
  }
});

test("a selection and an address longer than their limits are cut, say so, and are not cut through a character", () => {
  const long = itemFrom(
    { ...SELECTION, selectionText: "x".repeat(MAX_TEXT + 1) },
    NOW,
  );
  assert.equal(long.text.length, MAX_TEXT);
  assert.equal(long.cut, true);

  const exact = itemFrom({ ...SELECTION, selectionText: "x".repeat(MAX_TEXT) }, NOW);
  assert.equal(exact.text.length, MAX_TEXT);
  assert.equal(exact.cut, undefined);

  // The limit falls between the two halves of a character, so one unit fewer is kept.
  const emoji = `x${"😀".repeat(MAX_TEXT)}`;
  const split = itemFrom({ ...SELECTION, selectionText: emoji }, NOW);
  assert.equal(split.text.length, MAX_TEXT - 1);
  assert.equal(split.text, split.text.toWellFormed());
  assert.equal(split.cut, true);

  const address = itemFrom(
    { ...LINK, linkUrl: `https://a.example/${"p".repeat(MAX_ADDRESS)}` },
    NOW,
  );
  assert.equal(address.address.length, MAX_ADDRESS);
  assert.equal(address.cut, true);
  const page = itemFrom({ ...TAB, pageUrl: `https://a.example/${"q".repeat(MAX_ADDRESS)}` }, NOW);
  assert.equal(page.page.length, MAX_ADDRESS);
  assert.equal(page.cut, true);
});

test("the limits are the ones the extension and its pages say", () => {
  assert.equal(MAX_ITEMS, 10);
  assert.equal(MAX_TEXT, 20_000);
  assert.equal(MAX_ADDRESS, 2_000);
  assert.equal(MAX_AGE_MS, 30 * 60 * 1000);
});

test("items are kept oldest first in session storage, and taking them returns them and empties it", async () => {
  const chrome = fakeChrome();
  assert.equal(await holdItem(chrome, SELECTION, NOW), true);
  assert.equal(await holdItem(chrome, LINK, NOW + 1000), true);
  assert.equal(await holdItem(chrome, TAB, NOW + 2000), true);
  assert.deepEqual(
    kept(chrome).items.map((item) => item.kind),
    ["selection", "link", "tab"],
  );
  assert.equal(await heldCount(chrome, NOW + 3000), 3);
  // Nothing is written to the storage that is kept on disk.
  assert.deepEqual(chrome.local, {});

  const taken = await takeHeld(chrome, NOW + 3000);
  assert.deepEqual(
    taken.items.map((item) => item.kind),
    ["selection", "link", "tab"],
  );
  assert.equal(taken.forgotten, 0);
  assert.ok(!(HELD_KEY in chrome.session));
  assert.deepEqual(await takeHeld(chrome, NOW + 3000), { items: [], forgotten: 0 });
});

test("holding an item the entry made nothing for keeps nothing and writes nothing", async () => {
  const chrome = fakeChrome();
  assert.equal(await holdItem(chrome, { ...SELECTION, selectionText: "" }, NOW), false);
  assert.ok(!(HELD_KEY in chrome.session));
});

test("an eleventh item forgets the oldest, and the take says one was forgotten", async () => {
  const chrome = fakeChrome();
  for (let number = 0; number <= MAX_ITEMS; number += 1) {
    await holdItem(chrome, { ...SELECTION, selectionText: `item ${number}` }, NOW + number);
  }
  assert.equal(kept(chrome).items.length, MAX_ITEMS);
  assert.equal(kept(chrome).items[0].text, "item 1");
  const taken = await takeHeld(chrome, NOW + 100);
  assert.deepEqual(
    taken.items.map((item) => item.text),
    Array.from({ length: MAX_ITEMS }, (_, index) => `item ${index + 1}`),
  );
  assert.equal(taken.forgotten, 1);
});

test("an item older than the limit is forgotten when the next is held and is not returned by a take, and the take counts it", async () => {
  const chrome = fakeChrome();
  await holdItem(chrome, { ...SELECTION, selectionText: "old" }, NOW);
  await holdItem(chrome, { ...SELECTION, selectionText: "new" }, NOW + MAX_AGE_MS + 1);
  assert.deepEqual(
    kept(chrome).items.map((item) => item.text),
    ["new"],
  );
  assert.equal(kept(chrome).forgotten, 1);

  const second = fakeChrome();
  await holdItem(second, { ...SELECTION, selectionText: "edge" }, NOW);
  await holdItem(second, { ...SELECTION, selectionText: "late" }, NOW + 10);
  const taken = await takeHeld(second, NOW + MAX_AGE_MS + 5);
  assert.deepEqual(
    taken.items.map((item) => item.text),
    ["late"],
    "an item exactly as old as the limit is kept, and the one a moment older is not",
  );
  assert.equal(taken.forgotten, 1);
  assert.equal(await heldCount(second, NOW + MAX_AGE_MS + 5), 0);
});

test("an item made after the present, which a clock set back leaves, is not kept", async () => {
  const chrome = fakeChrome();
  await holdItem(chrome, SELECTION, NOW + 60_000);
  assert.equal(await heldCount(chrome, NOW), 0);
  const taken = await takeHeld(chrome, NOW);
  assert.deepEqual(taken, { items: [], forgotten: 1 });
});

test("forgetting empties what is kept, and a record that cannot be read holds nothing", async () => {
  const chrome = fakeChrome();
  await holdItem(chrome, SELECTION, NOW);
  await forgetHeld(chrome);
  assert.ok(!(HELD_KEY in chrome.session));
  assert.deepEqual(await takeHeld(chrome, NOW), { items: [], forgotten: 0 });

  for (const record of [
    "text",
    7,
    null,
    [],
    { items: "text" },
    { items: [null, 7, { kind: "nothing", time: NOW }, { kind: "tab", time: "late" }] },
    { items: [{ kind: "selection", time: NOW, text: 7 }] },
  ]) {
    const damaged = fakeChrome({ session: { [HELD_KEY]: record } });
    assert.deepEqual(
      await takeHeld(damaged, NOW),
      { items: [], forgotten: 0 },
      JSON.stringify(record),
    );
  }
});

test("two items held at once are both kept", async () => {
  const chrome = fakeChrome();
  await Promise.all([
    holdItem(chrome, { ...SELECTION, selectionText: "first" }, NOW),
    holdItem(chrome, { ...SELECTION, selectionText: "second" }, NOW + 1),
    holdItem(chrome, { ...SELECTION, selectionText: "third" }, NOW + 2),
  ]);
  assert.deepEqual(
    kept(chrome).items.map((item) => item.text),
    ["first", "second", "third"],
  );
});

test("the entries are made after the old ones are removed, once each, and removed when the group is off", async () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  await showMenus(chrome, true);
  await showMenus(chrome, true);
  assert.deepEqual(
    [...menus.entries.keys()],
    ["bravebot-hold-selection", "bravebot-hold-link", "bravebot-hold-tab"],
  );
  assert.deepEqual(menus.calls.map(([name]) => name.replace("contextMenus.", "")), [
    "removeAll",
    "create",
    "create",
    "create",
    "removeAll",
    "create",
    "create",
    "create",
  ]);
  assert.deepEqual(
    [...menus.entries.values()].map((entry) => [entry.title, entry.contexts]),
    MENU.map((entry) => [entry.title, entry.contexts]),
  );
  await showMenus(chrome, false);
  assert.equal(menus.entries.size, 0);
});

test("the entries are not made twice by calls that overlap", async () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  await Promise.all([showMenus(chrome, true), showMenus(chrome, true), showMenus(chrome, false)]);
  assert.equal(menus.entries.size, 0);
  const names = menus.calls.map(([name]) => name.replace("contextMenus.", ""));
  assert.deepEqual(names, [
    "removeAll",
    "create",
    "create",
    "create",
    "removeAll",
    "create",
    "create",
    "create",
    "removeAll",
  ]);
});

test("a browser with no context menu API is left alone, and what it reports is the failure", async () => {
  const none = fakeChrome();
  await showMenus(none, true);

  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  menus.menus.failWith = "the menu is busy";
  await assert.rejects(showMenus(chrome, true), /the menu is busy/);
  assert.equal(menus.entries.size, 0);
  // The next call is not held up by the one that failed.
  await showMenus(chrome, true);
  assert.equal(menus.entries.size, MENU.length);
});

// The browser reports a use of an entry to a listener registered as the worker
// starts, and the group has to be on for it to hold anything.
test("a use of an entry holds an item while the group is on, and holds none while it is off", async () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  let on = true;
  listenForHolds(chrome, async () => on);
  menus.click(SELECTION);
  await flush();
  assert.equal(kept(chrome).items.length, 1);

  on = false;
  menus.click(LINK);
  await flush();
  assert.equal(kept(chrome).items.length, 1, "an entry that outlived its group held an item");

  on = true;
  menus.click({ ...TAB, menuItemId: "another-extensions-entry" });
  await flush();
  assert.equal(kept(chrome).items.length, 1);
});

// The group is turned off while the answer to "may an item be held" is on its
// way: the turn-off has already forgotten what was held, the item is then written,
// and nothing would forget it again. The listener asks once more after the write.
test("an item written after the group was turned off is forgotten, and none stays held", async () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  let on = true;
  let answers = 0;
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  listenForHolds(chrome, async () => {
    answers += 1;
    if (answers === 1) {
      // The first answer was read while the group was on and arrives late.
      await gate;
      return true;
    }
    return on;
  });
  menus.click(SELECTION);
  await flush();
  // The group is turned off and everything held is forgotten, as turning it off
  // does, before the late answer arrives.
  on = false;
  await forgetHeld(chrome);
  release();
  await flush();
  await flush();
  assert.equal(await heldCount(chrome), 0, "an item was held for a group that is off");
  assert.ok(!(HELD_KEY in chrome.session));

  // An item held while the group stays on is kept.
  on = true;
  answers = 1;
  menus.click(LINK);
  await flush();
  await flush();
  assert.equal(kept(chrome).items.length, 1);
  assert.equal(kept(chrome).items[0].kind, "link");
});

// The address of the page is cut at MAX_ADDRESS, so the site it names is read
// from the whole address before it is cut.
test("an item remembers the origin of the whole address of its page, though the address is cut", () => {
  const long = `https://${"a".repeat(MAX_ADDRESS + 100)}@bank.example/account`;
  const item = itemFrom({ ...TAB, pageUrl: long }, NOW);
  assert.equal(item.page.length, MAX_ADDRESS);
  assert.equal(item.cut, true);
  assert.equal(item.origin, "https://bank.example");
  assert.equal(itemFrom(TAB, NOW).origin, "https://news.example");
  // A page with no origin has none recorded.
  assert.ok(!("origin" in itemFrom({ ...TAB, pageUrl: "about:blank" }, NOW)));
  assert.ok(!("origin" in itemFrom({ menuItemId: "bravebot-hold-selection", selectionText: "x" }, NOW)));
});

test("the listener is registered once for a browser, and not at all where there is no context menu API", () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  listenForHolds(chrome, async () => true);
  listenForHolds(chrome, async () => true);
  assert.equal(menus.listeners.length, 1);

  const none = fakeChrome();
  listenForHolds(none, async () => true);
  assert.ok(!("contextMenus" in none));
});

test("a group that cannot be asked about holds nothing", async () => {
  const chrome = fakeChrome();
  const menus = withMenus(chrome);
  const quiet = [];
  const note = console.info;
  console.info = (...words) => quiet.push(words);
  try {
    listenForHolds(chrome, async () => {
      throw new Error("the switches cannot be read");
    });
    menus.click(SELECTION);
    await flush();
  } finally {
    console.info = note;
  }
  assert.ok(!(HELD_KEY in chrome.session));
  assert.match(quiet.flat().join(" "), /the switches cannot be read/);
});

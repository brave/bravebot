// Numbering the elements of a page: list_page_elements gives each element a
// number, and the input tools accept that number, which a person read off the
// list, in place of a role and a name. The functions the extension injects run
// against a fake page (fake-dom.mjs), the confirmation window is a double that
// answers as a test says, and the browser is a fake that records every call.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { activityLog } from "../activity.js";
import { approve, setConfirmation } from "../approval.js";
import { GROUPS, SCOPE_ALL } from "../groups.js";
import { FAILURES, OUTLINE_MS, inPage } from "../input.js";
import { LIFETIMES_KEY, RUNS_KEY } from "../lifetimes.js";
import {
  FINGERPRINT_LIMIT,
  LISTED_NAME_LIMIT,
  MAX_AGE_MS,
  MAX_LISTED,
  NOT_FOUND,
  NUMBERS_KEY,
} from "../numbering.js";
import {
  GROUP_SWITCHES_KEY,
  turnEverythingOff,
  turnOff,
  turnOffLapsed,
  turnOn,
} from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
  handle,
} from "../tools.js";
import { el, eventsOf, kindsOn, page, shadowed } from "./fake-dom.mjs";
import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

const SHOP = "https://shop.example/cart";
const OTHER = "https://other.example/home";
const ORIGIN = "https://shop.example";
const NUMBERED = [...GROUPS["page-read"].tools, ...GROUPS.input.tools];

// A fake browser with the page-read and input groups on, on all sites, and every
// permission granted. `fakes` maps a tab id to its page, and `tabs` lists the
// tabs. `session` is added to what session storage holds. `injected` keeps the
// options of every executeScript call.
function browser({
  fakes,
  tabs = Object.keys(fakes).map((id) => ({ id: Number(id), windowId: 1, title: "T", url: fakes[id].href })),
  stored = {},
  session = {},
} = {}) {
  const calls = [];
  const injected = [];
  const local = {
    [SETTINGS_KEY]: Object.fromEntries(NUMBERED.map((tool) => [tool, true])),
    [GROUP_SWITCHES_KEY]: { "page-read": true, input: true },
    [SITE_SCOPES_KEY]: { "page-read": SCOPE_ALL, input: SCOPE_ALL },
    ...stored,
  };
  return {
    calls,
    injected,
    local,
    permissions: fakePermissions({ held: allOptional(), calls }),
    storage: {
      local: fakeArea(local),
      session: fakeArea({ [RUNS_KEY]: { input: true, "page-read": true }, ...session }),
    },
    tabs: {
      async query() {
        calls.push(["tabs.query"]);
        return tabs;
      },
      async update(id, properties) {
        calls.push(["tabs.update", id, properties]);
        return tabs.find((tab) => tab.id === id);
      },
    },
    scripting: {
      async executeScript(options) {
        calls.push(["scripting.executeScript", options.target.tabId]);
        injected.push(options);
        return [
          {
            frameId: 0,
            result: fakes[options.target.tabId].run(() => options.func(...options.args)),
          },
        ];
      },
    },
  };
}

const call = (chrome, method, params) => handle({ id: 1, method, params }, chrome);

const stored = (chrome) => chrome.storage.session.data[NUMBERS_KEY];

// The person's window: it records what it was shown and answers with a token for
// it, unless `answer` says otherwise.
function window(answer = approve) {
  const asked = [];
  setConfirmation(async (resolution) => {
    asked.push(resolution);
    return answer(resolution);
  });
  return asked;
}

afterEach(() => {
  setConfirmation();
  mock.restoreAll();
});

const shop = (body, href = SHOP) => page({ href, body });

const button = (id, name, more = {}) => el("button", { id }, [name], more);

// The page's events at the moment of the answer, without scrolling and focus.
const happenings = (fake) =>
  fake.events.filter((event) => !["scrollIntoView", "focus"].includes(event.type));

// The rows of a list as a person reads them: number, role, name, goes to.
const rowsOf = (text) =>
  text
    .split("\n")
    .slice(2)
    .map((line) => line.split("\t"));

// The number a person reads off the list for the element with this role and
// name.
function numberOf(text, role, name) {
  const row = rowsOf(text).find((cells) => cells[1] === role && cells[2] === name);
  assert.ok(row, `the list has no ${role} named ${name}:\n${text}`);
  return Number(row[0]);
}

const list = async (chrome, url = SHOP) => {
  const reply = await call(chrome, "list_page_elements", { url });
  assert.equal(typeof reply.result, "string", JSON.stringify(reply));
  return reply.result;
};

test("list_page_elements numbers the controls of the page in document order, as text of a role, a name and where it goes, and the same page gives the same numbers", async () => {
  const fake = shop([
    el("h1", {}, ["Cart"]),
    el("a", { href: "/" }, ["Home"], { href: "https://shop.example/" }),
    el("p", {}, ["Not a control"]),
    el("a", { href: "/out" }, ["Leave"], {
      href: "https://elsewhere.example/a/b?token=secret#frag",
    }),
    el("input", { type: "text", "aria-label": "Search" }),
    button("pay", "Pay now"),
    el("a", { href: "javascript:void(0)" }, ["Menu"], { href: "javascript:void(0)" }),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const first = await list(chrome);
  const [summary, header, ...rows] = first.split("\n");
  assert.match(summary, /^Elements numbered: 5, in document order\./);
  assert.match(summary, /element_number to click_element, type_text or select_option/);
  assert.match(summary, /holds for 5 minutes, or until another list is made\.$/);
  assert.equal(header, "number\trole\tname\tgoes to");
  assert.deepEqual(rows, [
    "1\tlink\tHome\thttps://shop.example",
    "2\tlink\tLeave\thttps://elsewhere.example",
    "3\ttextbox\tSearch",
    "4\tbutton\tPay now",
    "5\tlink\tMenu\tjavascript:",
  ]);
  assert.ok(!first.includes("secret"), "a query reached the list");
  assert.equal(await list(chrome), first);
});

test("a list is cut to 40 elements and 120 characters of each name, says how many it left out, and keeps no more than it numbered", async () => {
  assert.equal(MAX_LISTED, 40);
  assert.equal(LISTED_NAME_LIMIT, 120);
  const long = "w".repeat(300);
  const fake = shop([
    button("long", long),
    ...Array.from({ length: 44 }, (_, at) => button(`b${at}`, `Button ${at}`)),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const text = await list(chrome);
  const rows = rowsOf(text);
  assert.equal(rows.length, 40);
  assert.deepEqual(
    rows.map((cells) => cells[0]),
    Array.from({ length: 40 }, (_, at) => String(at + 1)),
  );
  assert.equal(rows[0][2], `${"w".repeat(120)}...`);
  assert.equal(rows[39][2], "Button 38");
  assert.match(text.split("\n")[0], /Not listed: 5 beyond the first 40\./);
  const kept = stored(chrome);
  assert.equal(kept.elements.length, 40);
  assert.equal(kept.url, SHOP);
});

test("an element whose name or destination is too long to compare is left out and counted, and the numbers go on without a gap", async () => {
  // BROWSER-41 says 2,000 characters.
  assert.equal(FINGERPRINT_LIMIT, 2_000);
  const fake = shop([
    button("a", "First"),
    button("huge", "h".repeat(2_001)),
    button("b", "Second"),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const text = await list(chrome);
  assert.deepEqual(
    rowsOf(text).map((cells) => cells.slice(0, 3)),
    [
      ["1", "button", "First"],
      ["2", "button", "Second"],
    ],
  );
  assert.match(text.split("\n")[0], /Not listed: 1 with a name or destination too long to compare\./);
  assert.equal(stored(chrome).elements.length, 2);
});

// A list is the page's controls and not what the person has typed into them.
test("the list holds the role and name of a field and none of its value, leaves out the fields never acted on and the controls that are not shown, and counts them", async () => {
  const reads = [];
  const field = (attributes, held, more = {}) => {
    const made = el("input", attributes, [], more);
    Object.defineProperty(made, "value", {
      get() {
        reads.push(attributes["aria-label"]);
        return held;
      },
    });
    return made;
  };
  const fake = shop([
    field({ type: "text", "aria-label": "Full name" }, "Ada Lovelace"),
    field({ type: "password", "aria-label": "Password" }, "hunter2"),
    field({ type: "text", autocomplete: "cc-number", "aria-label": "Card number" }, "4111 1111 1111 1111"),
    field({ type: "file", "aria-label": "Resume" }, "C:\\fakepath\\cv.pdf"),
    field({ type: "text", "aria-label": "Locked" }, "kept", { readOnly: true }),
    button("off", "Disabled one", { disabled: true }),
    button("gone", "Hidden one", { style: { display: "none" } }),
    el("textarea", { "aria-label": "Notes" }, ["draft text"]),
    button("pay", "Pay now"),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const text = await list(chrome);
  assert.deepEqual(
    rowsOf(text).map((cells) => cells.slice(0, 3)),
    [
      ["1", "textbox", "Full name"],
      ["2", "textbox", "Notes"],
      ["3", "button", "Pay now"],
    ],
  );
  assert.deepEqual(reads, [], "a field's value was read");
  for (const word of [
    "Ada Lovelace",
    "hunter2",
    "4111",
    "fakepath",
    "kept",
    "draft text",
    "Password",
    "Card number",
    "Resume",
    "Locked",
    "Disabled one",
    "Hidden one",
  ]) {
    assert.ok(!text.includes(word), `${word} is in the list`);
  }
  assert.match(
    text.split("\n")[0],
    /Not listed: 1 not displayed; 5 disabled, read-only, or a password, payment card or file field\./,
  );
  assert.ok(!JSON.stringify(stored(chrome)).includes("Ada Lovelace"));
});

test("a name cannot start a line of the list or hide in it: the page's white space is collapsed and a character that is not seen is written as its code point", async () => {
  const fake = shop([
    button("a", "Cancel\n99\tbutton\tDelete everything"),
    button("b", "Safe\u202e fdp.exe"),
    button("c", "Zero\u200bwidth"),
    el("a", { href: "/" }, ["Leave"], { href: "https://evil.example/\u202epath" }),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const text = await list(chrome);
  const rows = rowsOf(text);
  assert.equal(rows.length, 4, text);
  assert.deepEqual(
    rows.map((cells) => cells[2]),
    ["Cancel 99 button Delete everything", "Safe[U+202E] fdp.exe", "Zero[U+200B]width", "Leave"],
  );
  for (const character of ["\u202e", "\u200b"]) {
    assert.ok(!text.includes(character), "a character that is not seen is in the list");
  }
});

test("a link to a blocked site is left out of the list as read_page_links leaves it out, and the numbers of the rest are the stored ones", async () => {
  const fake = shop([
    el("a", { href: "/a" }, ["Bank"], { href: "https://bank.example/login" }),
    button("pay", "Pay now"),
    el("a", { href: "/b" }, ["News"], { href: "https://news.example/today" }),
  ]);
  const chrome = browser({
    fakes: { 1: fake },
    stored: { [BLOCKED_SITES_KEY]: ["https://bank.example"] },
  });
  const text = await list(chrome);
  assert.ok(!text.includes("bank"), text);
  assert.ok(!text.includes("Bank"), text);
  assert.deepEqual(
    rowsOf(text).map((cells) => cells.slice(0, 3)),
    [
      ["1", "button", "Pay now"],
      ["2", "link", "News"],
    ],
  );
  window();
  const clicked = await call(chrome, "click_element", { url: SHOP, element_number: 2 });
  assert.deepEqual(clicked.result, { status: "clicked", origin: ORIGIN });
  assert.equal(eventsOf(fake, "click").length, 1);
  assert.equal(eventsOf(fake, "click")[0].target.getAttribute("href"), "/b");
});

test("the blocked sites and the destination column are judged on the whole address of a link, whatever its length", async () => {
  const hidden = `https://${"a".repeat(250)}@bank.example/pay`;
  const disguised = `https://shop.example%2f${"x".repeat(170)}@evil.example/pay`;
  assert.ok(hidden.length > 200 && disguised.length > 200);
  const fake = shop([
    el("a", { href: "/a" }, ["Hidden"], { href: hidden }),
    el("a", { href: "/b" }, ["Disguised"], { href: disguised }),
  ]);
  const chrome = browser({
    fakes: { 1: fake },
    stored: { [BLOCKED_SITES_KEY]: ["https://bank.example"] },
  });
  const rows = rowsOf(await list(chrome));
  assert.deepEqual(rows, [["1", "link", "Disguised", "https://evil.example"]]);
  window();
  const clicked = await call(chrome, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(clicked.result?.status, "clicked");
  assert.equal(stored(chrome).elements.length, 1);
});

test("listing a page refuses what the other page tools refuse: a blocked site, a site not allowed, no tab and two tabs, before any page is reached", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const blocked = browser({ fakes: { 1: fake }, stored: { [BLOCKED_SITES_KEY]: [ORIGIN] } });
  assert.match((await call(blocked, "list_page_elements", { url: SHOP })).error.message, /blocked sites list/);
  const limited = browser({
    fakes: { 1: fake },
    stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: ["https://other.example"] },
  });
  assert.match((await call(limited, "list_page_elements", { url: SHOP })).error.message, /not in the allowed sites list/);
  const none = browser({ fakes: { 1: fake }, tabs: [{ id: 1, url: `${SHOP}?a=1` }] });
  assert.equal((await call(none, "list_page_elements", { url: SHOP })).error.message, `no open tab is at ${SHOP}`);
  const twice = browser({ fakes: { 1: fake }, tabs: [{ id: 1, url: SHOP }, { id: 2, url: SHOP }] });
  assert.match((await call(twice, "list_page_elements", { url: SHOP })).error.message, /2 open tabs are at/);
  const left = browser({ fakes: { 1: fake } });
  fake.href = "https://elsewhere.example/";
  assert.equal((await call(left, "list_page_elements", { url: SHOP })).error.message, FAILURES.left());
  for (const chrome of [blocked, limited, none, twice, left]) {
    assert.equal(stored(chrome), undefined);
  }
  for (const chrome of [blocked, limited, none, twice]) {
    assert.ok(!chrome.calls.some(([name]) => name === "scripting.executeScript"));
  }
});

test("a number leads to the element it was given to: the same role and name twice is told apart by number, and each tool that acts takes one", async () => {
  const fake = shop([
    button("first", "Buy"),
    button("second", "Buy"),
    el("input", { id: "who", type: "text", "aria-label": "Name" }),
    el("select", { id: "size", "aria-label": "Size" }, [
      el("option", {}, ["Small"]),
      el("option", {}, ["Large"]),
    ]),
  ]);
  const chrome = browser({ fakes: { 1: fake } });
  const asked = window();
  const text = await list(chrome);
  const buys = rowsOf(text).filter((cells) => cells[2] === "Buy").map((cells) => Number(cells[0]));
  assert.deepEqual(buys, [1, 2]);

  const byName = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Buy" });
  assert.match(byName.error.message, /2 elements have that role and that name/);

  const second = await call(chrome, "click_element", { url: SHOP, element_number: 2 });
  assert.deepEqual(second.result, { status: "clicked", origin: ORIGIN });
  assert.deepEqual(kindsOn(fake, fake.element("first")), []);
  assert.ok(kindsOn(fake, fake.element("second")).includes("click"));
  assert.equal(asked.at(-1).description.action, "click");
  assert.equal(asked.at(-1).description.name, "Buy");
  assert.deepEqual(asked.at(-1).description.rect, { x: 10, y: 50, width: 100, height: 30 });

  const typed = await call(chrome, "type_text", {
    url: SHOP,
    element_number: numberOf(text, "textbox", "Name"),
    text: "Ada",
  });
  assert.deepEqual(typed.result, { status: "typed", origin: ORIGIN });
  assert.equal(fake.element("who").value, "Ada");
  assert.equal(asked.at(-1).description.text, "Ada");

  const chosen = await call(chrome, "select_option", {
    url: SHOP,
    element_number: numberOf(text, "combobox", "Size"),
    option: "Large",
  });
  assert.deepEqual(chosen.result, { status: "selected", origin: ORIGIN });
  assert.equal(asked.at(-1).description.option, "Large");
});

test("the window is shown the element a number leads to as it is shown for a role and a name, and nothing happens before the person allows", async () => {
  const make = () => {
    const form = el("form", {}, [], { action: "https://shop.example/charge" });
    return shop([
      form,
      button("buy", "Buy", { form }),
      el("a", { href: "/out" }, ["Leave"], { href: "https://elsewhere.example/x" }),
    ]);
  };
  const byName = make();
  const named = browser({ fakes: { 1: byName } });
  const seenByName = window(() => null);
  await call(named, "click_element", { url: SHOP, role: "button", name: "Buy" });

  const fake = make();
  const chrome = browser({ fakes: { 1: fake } });
  let before;
  const asked = window((resolution) => {
    before = happenings(fake);
    return approve(resolution);
  });
  const text = await list(chrome);
  const reply = await call(chrome, "click_element", {
    url: SHOP,
    element_number: numberOf(text, "button", "Buy"),
  });
  assert.deepEqual(reply.result, { status: "clicked", origin: ORIGIN });
  assert.deepEqual(before, [], "something happened before the person answered");
  // The window is told the number too, and which of the elements of that role
  // and name it is; the rest is what a role and a name give.
  const { number, position, ...described } = asked[0].description;
  assert.deepEqual(described, seenByName[0].description);
  assert.equal(number, numberOf(text, "button", "Buy"));
  assert.deepEqual(position, { index: 1, of: 1 });
  assert.equal("number" in seenByName[0].description, false);
  assert.equal("position" in seenByName[0].description, false);
  assert.equal(asked[0].description.goes, "https://shop.example/charge");
  // The element was outlined for the window, and the outline is gone after.
  assert.equal(fake.shadows.length, 1);
  assert.deepEqual(fake.added(), []);
  assert.equal(chrome.injected.find((made) => made.args[0].outline > 0).args[0].outline, OUTLINE_MS);

  const denied = window(() => null);
  const refused = await call(chrome, "click_element", {
    url: SHOP,
    element_number: numberOf(text, "link", "Leave"),
  });
  assert.equal(denied.length, 1);
  assert.match(refused.error.message, /not allowed in the browser/);
  assert.equal(eventsOf(fake, "click").length, 1);
});

// Two elements with one role and name differ in the list by their numbers and
// in the window by the number and the place among the elements of their name,
// since their words are the same.
test("the window is told the number of the element and which of the displayed elements of its role and name it is, so that two that look alike are told apart", async () => {
  const fake = shop([
    button("first", "Buy"),
    button("second", "Buy"),
    button("hidden", "Buy"),
    el("input", { type: "text", "aria-label": "Buy" }),
    button("third", "Buy"),
    button("other", "Pay"),
  ]);
  fake.element("hidden").style.display = "none";
  const chrome = browser({ fakes: { 1: fake } });
  const asked = window();
  const text = await list(chrome);
  const buys = rowsOf(text)
    .filter((cells) => cells[1] === "button" && cells[2] === "Buy")
    .map((cells) => Number(cells[0]));
  assert.equal(buys.length, 3, text);
  for (const number of [...buys].reverse()) {
    await call(chrome, "click_element", { url: SHOP, element_number: number });
  }
  assert.deepEqual(
    asked.map(({ description }) => [description.number, description.position]),
    [
      [buys[2], { index: 3, of: 3 }],
      [buys[1], { index: 2, of: 3 }],
      [buys[0], { index: 1, of: 3 }],
    ],
  );
  // The descriptions are otherwise one but for the rectangle, which the window
  // does not show: nothing but the number and the place tells the three buttons
  // apart in the window.
  const alike = asked.map(
    ({ description: { number: n, position: p, rect, ...rest } }) => rest,
  );
  assert.deepEqual(alike[0], alike[1]);
  assert.deepEqual(alike[1], alike[2]);
  // An element of another name is the only one of its own.
  await call(chrome, "click_element", { url: SHOP, element_number: numberOf(text, "button", "Pay") });
  assert.deepEqual(asked.at(-1).description.position, { index: 1, of: 1 });
  // A button named by its role and name is shown with no number.
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay" });
  assert.equal("number" in asked.at(-1).description, false);
  assert.equal("position" in asked.at(-1).description, false);
  // Neither is a word of the page: they are whole numbers.
  for (const { description } of asked) {
    if ("number" in description) {
      assert.ok(Number.isInteger(description.number) && description.number >= 1);
      assert.ok(Number.isInteger(description.position.index) && Number.isInteger(description.position.of));
    }
  }
});

// What the page is asked: the place and the fingerprint that the list kept, and
// nothing the planner wrote.
test("the page is given the place and the fingerprint the list kept for the number, and no word of the call", async () => {
  const fake = shop([button("pay", "Pay now"), button("later", "Pay later")]);
  const chrome = browser({ fakes: { 1: fake } });
  window();
  await list(chrome);
  const kept = stored(chrome).elements[1];
  chrome.injected.length = 0;
  await call(chrome, "click_element", {
    url: SHOP,
    element_number: 2,
    selector: "#pay",
    name: undefined,
    x: 3,
    script: "alert(1)",
  });
  const [find, clear, act] = chrome.injected;
  assert.deepEqual(Object.keys(find.args[0]).sort(), ["at", "expect", "limit", "outline", "step", "url"]);
  assert.equal(find.args[0].at, kept.path);
  assert.equal(find.args[0].expect, kept.fingerprint);
  assert.deepEqual(Object.keys(clear.args[0]), ["url"]);
  assert.deepEqual(
    Object.keys(act.args[0]).sort(),
    ["action", "at", "expect", "fingerprint", "limit", "step", "text", "url"],
  );
  assert.equal(chrome.injected.length, 3);
});

// Each of these leaves the page with an element at the number's place that the
// person was never shown a number for.
test("an element that is not the one that was listed is refused with the same sentence, nothing is done and the person is not asked: swapped, renamed, retargeted, hidden, moved, removed", async () => {
  const swap = (fake, id, made) => {
    const old = fake.element(id);
    made.parentNode = old.parentNode;
    old.parentNode.childNodes[old.parentNode.childNodes.indexOf(old)] = made;
  };
  const changes = {
    "swapped for a button with another name": (fake) =>
      swap(fake, "pay", el("button", { id: "evil" }, ["Delete account"])),
    "swapped for an element of another tag with the same name": (fake) =>
      swap(fake, "pay", el("a", { id: "evil", role: "button", href: "/" }, ["Pay now"])),
    "renamed": (fake) => {
      fake.element("pay").attributes["aria-label"] = "Cancel order";
    },
    "a link sent to another address": (fake) => {
      fake.element("go").href = "https://evil.example/";
    },
    "a button whose form is sent to another address": (fake) => {
      fake.element("pay").form = el("form", {}, [], { action: "https://evil.example/charge" });
    },
    "hidden": (fake) => {
      fake.element("pay").style.display = "none";
    },
    "moved by an element put before it": (fake) => {
      const body = fake.element("pay").parentNode;
      const made = el("p", {}, ["Sale!"]);
      made.parentNode = body;
      body.childNodes.unshift(made);
    },
    "removed": (fake) => {
      const gone = fake.element("pay");
      gone.parentNode.childNodes.splice(gone.parentNode.childNodes.indexOf(gone), 1);
    },
  };
  for (const [why, change] of Object.entries(changes)) {
    const fake = shop([
      el("a", { id: "go", href: "/go" }, ["Continue"], { href: "https://shop.example/next" }),
      button("pay", "Pay now"),
    ]);
    const chrome = browser({ fakes: { 1: fake } });
    const asked = window();
    const text = await list(chrome);
    const number = numberOf(text, "button", "Pay now");
    const link = numberOf(text, "link", "Continue");
    change(fake);
    fake.events.length = 0;
    const target = why.startsWith("a link") ? link : number;
    const reply = await call(chrome, "click_element", { url: SHOP, element_number: target });
    assert.equal(reply.error?.message, FAILURES.stale(), why);
    assert.ok(!("result" in reply), why);
    assert.equal(asked.length, 0, `${why}: the person was asked`);
    assert.deepEqual(happenings(fake), [], `${why}: something happened`);
    assert.deepEqual(fake.added(), [], `${why}: an outline was drawn`);
    assert.equal((await activityLog(chrome)).at(-1).outcome, "refused", why);
  }
});

test("the page changing while the window waits is refused as well, and so is a field turned into a password field after it was listed", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  const asked = window((resolution) => {
    fake.element("pay").attributes["aria-label"] = "Cancel order";
    return approve(resolution);
  });
  const text = await list(chrome);
  const reply = await call(chrome, "click_element", {
    url: SHOP,
    element_number: numberOf(text, "button", "Pay now"),
  });
  assert.equal(asked.length, 1);
  assert.equal(reply.error?.message, FAILURES.stale());
  assert.deepEqual(happenings(fake), []);

  // The type of a field is not part of the fingerprint, so the field is found
  // as listed, and it is refused for what it is now.
  const secret = shop([el("input", { id: "code", type: "text", "aria-label": "Code" })]);
  const typing = browser({ fakes: { 1: secret } });
  const none = window();
  const listed = await list(typing);
  secret.element("code").attributes.type = "password";
  const refused = await call(typing, "type_text", {
    url: SHOP,
    element_number: numberOf(listed, "textbox", "Code"),
    text: "123456",
  });
  assert.equal(refused.error?.message, FAILURES.password());
  assert.equal(none.length, 0);
  assert.deepEqual(happenings(secret), []);
  assert.equal((await activityLog(typing)).at(-1).outcome, "refused");
});

test("a page that left its URL after the list is refused as every action on it is, and a list is not made of a page that left", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  const asked = window();
  await list(chrome);
  fake.href = "https://elsewhere.example/";
  const reply = await call(chrome, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(reply.error?.message, FAILURES.left());
  assert.equal(asked.length, 0);
  assert.deepEqual(happenings(fake), []);
});

test("a number with no list, a list for another page, a list that is too old, a number past the end of the list and a list that cannot be read are each refused with a fixed sentence, before a page is reached", async () => {
  const fakeA = shop([button("a", "Alpha"), button("b", "Beta")]);
  const fakeB = shop([button("c", "Gamma")], OTHER);
  const asked = window();
  const reached = (chrome) => chrome.injected.length;

  // No list was made.
  const fresh = browser({ fakes: { 1: fakeA } });
  const none = await call(fresh, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(none.error?.message, NOT_FOUND.none);
  assert.equal(reached(fresh), 0);

  // A list was made for a tab, and a number is given for another page.
  const two = browser({ fakes: { 1: fakeA, 2: fakeB } });
  await list(two);
  const other = await call(two, "click_element", { url: OTHER, element_number: 1 });
  assert.equal(other.error?.message, NOT_FOUND.other);
  const inAnother = two.injected.length;
  assert.equal(inAnother, 1, "the other page was reached");

  // A second list replaces the first: it is for the other page now.
  await list(two, OTHER);
  assert.equal(stored(two).url, OTHER);
  const replaced = await call(two, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(replaced.error?.message, NOT_FOUND.other);
  const onlyOne = Object.keys(two.storage.session.data).filter((key) => key.startsWith(NUMBERS_KEY));
  assert.deepEqual(onlyOne, [NUMBERS_KEY]);
  assert.equal(stored(two).elements.length, 1);

  // A number past the end of the list.
  const short = browser({ fakes: { 1: fakeA } });
  await list(short);
  const past = await call(short, "type_text", { url: SHOP, element_number: 3, text: "x" });
  assert.equal(past.error?.message, NOT_FOUND.past(2));
  assert.equal(short.injected.length, 1);

  // A list that cannot be read is not a list.
  const base = Date.now();
  for (const [why, kept] of Object.entries({
    "not an object": "listed",
    "no address": { at: base, elements: [] },
    "no time": { url: SHOP, elements: [] },
    "more than the largest list": {
      url: SHOP,
      at: base,
      elements: Array.from({ length: MAX_LISTED + 1 }, () => ({ path: "0", fingerprint: "x" })),
    },
    "an element with no fingerprint": { url: SHOP, at: base, elements: [{ path: "0" }] },
    "a fingerprint past the limit": {
      url: SHOP,
      at: base,
      elements: [{ path: "0", fingerprint: "x".repeat(2_001) }],
    },
  })) {
    const broken = browser({ fakes: { 1: fakeA }, session: { [NUMBERS_KEY]: kept } });
    const reply = await call(broken, "select_option", { url: SHOP, element_number: 1, option: "x" });
    assert.equal(reply.error?.message, NOT_FOUND.none, why);
    assert.equal(broken.injected.length, 0, why);
  }
  assert.equal(asked.length, 0);
  for (const chrome of [fresh, two, short]) {
    assert.ok(
      (await activityLog(chrome)).every((entry) => entry.outcome !== "ran" || entry.tool === "list_page_elements"),
    );
  }
  assert.equal((await activityLog(fresh))[0].outcome, "refused");
});

test("a list is good for five minutes and no longer", async () => {
  assert.equal(MAX_AGE_MS, 5 * 60 * 1000);
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  window();
  const start = Date.now();
  const now = mock.method(Date, "now", () => start);
  await list(chrome);
  now.mock.mockImplementation(() => start + MAX_AGE_MS);
  const within = await call(chrome, "click_element", { url: SHOP, element_number: 1 });
  assert.deepEqual(within.result, { status: "clicked", origin: ORIGIN });
  now.mock.mockImplementation(() => start + MAX_AGE_MS + 1);
  const reached = chrome.injected.length;
  const late = await call(chrome, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(late.error?.message, NOT_FOUND.old);
  assert.match(late.error.message, /more than 5 minutes old/);
  assert.equal(chrome.injected.length, reached);
  // A clock that is behind the time of the list does not make it fresher.
  now.mock.mockImplementation(() => start - 1);
  const early = await call(chrome, "click_element", { url: SHOP, element_number: 1 });
  assert.equal(early.error?.message, NOT_FOUND.old);
});

test("the number is checked before a tab or a page is reached: it is a whole number from 1 to 40, and is given instead of a role and a name and not with them", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  const asked = window();
  await list(chrome);
  chrome.injected.length = 0;
  chrome.calls.length = 0;
  const base = { url: SHOP };
  const bad = [
    [{ element_number: 0 }, /element_number must be a whole number from 1 to 40/],
    [{ element_number: 41 }, /element_number must be a whole number from 1 to 40/],
    [{ element_number: 1.5 }, /element_number must be a whole number/],
    [{ element_number: "1" }, /element_number must be a whole number/],
    [{ element_number: null }, /element_number must be a whole number/],
    [{ element_number: true }, /element_number must be a whole number/],
    [{ element_number: 1, role: "button" }, /not both/],
    [{ element_number: 1, name: "Pay now" }, /not both/],
    [{ element_number: 1, role: "button", name: "Pay now" }, /not both/],
    [{}, /role must be one of/],
    [{ name: "Pay now" }, /role must be one of/],
  ];
  for (const [given, message] of bad) {
    for (const [method, more] of [
      ["click_element", {}],
      ["type_text", { text: "x" }],
      ["select_option", { option: "x" }],
    ]) {
      const reply = await call(chrome, method, { ...base, ...more, ...given });
      assert.match(reply.error?.message ?? "", message, `${method} ${JSON.stringify(given)}`);
      assert.equal(reply.error.code, -32602, `${method} ${JSON.stringify(given)}`);
    }
  }
  assert.equal(chrome.injected.length, 0);
  assert.ok(!chrome.calls.some(([name]) => name === "tabs.query"));
  assert.equal(asked.length, 0);

  // The other arguments of the call are checked as they are for a name.
  const noText = await call(chrome, "type_text", { ...base, element_number: 1 });
  assert.match(noText.error.message, /text/);
  const right = await call(chrome, "click_element", { ...base, element_number: 1, button: "right", count: 2 });
  assert.match(right.error.message, /a right click is made once/);
  assert.equal(chrome.injected.length, 0);
});

test("a tool that acts on no element, or on the element with the focus, takes no number", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  window();
  await list(chrome);
  const reached = chrome.injected.length;
  const waited = await call(chrome, "wait_for_element", { url: SHOP, element_number: 1, seconds: 1 });
  assert.match(waited.error.message, /role must be one of/);
  assert.equal(chrome.injected.length, reached);
  const key = await call(chrome, "press_key", { url: SHOP, key: "Enter", element_number: 1 });
  assert.equal(key.result.status, "pressed");
  const asked = chrome.injected.slice(reached).filter((made) => made.args[0].step !== undefined);
  assert.equal(asked.length, 2);
  for (const made of asked) {
    assert.ok(!("at" in made.args[0]) && !("expect" in made.args[0]));
    assert.equal(made.args[0].role, null);
  }
});

test("an element in a shadow root, open or closed, is numbered and acted on by its number", async () => {
  const closed = shadowed(el("div"), "closed", [button("deep", "Deep")]);
  const open = shadowed(el("div"), "open", [button("shallow", "Shallow")]);
  const fake = shop([button("top", "Top"), closed, open]);
  const chrome = browser({ fakes: { 1: fake } });
  window();
  const text = await list(chrome);
  assert.deepEqual(
    rowsOf(text).map((cells) => cells[2]).sort(),
    ["Deep", "Shallow", "Top"],
  );
  for (const [name, id] of [["Deep", "deep"], ["Shallow", "shallow"], ["Top", "top"]]) {
    fake.events.length = 0;
    const reply = await call(chrome, "click_element", {
      url: SHOP,
      element_number: numberOf(text, "button", name),
    });
    assert.equal(reply.result?.status, "clicked", name);
    assert.equal(eventsOf(fake, "click")[0].target, fake.element(id), name);
  }
});

// The page is only ever asked for a place together with the fingerprint the list
// recorded for it. This is the page's side of that: asked for a place alone, it
// finds nothing it will accept.
test("the page accepts an element found by its place only with the fingerprint the list gave, and a place with none is refused", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fakes: { 1: fake } });
  await list(chrome);
  const { path, fingerprint } = stored(chrome).elements[0];
  const ask = (extra) =>
    fake.run(() =>
      inPage({ url: SHOP, step: "find", at: path, limit: { name: 200, excerpt: 80, target: 200 }, outline: 0, ...extra }),
    );
  assert.equal(ask({ expect: fingerprint }).status, "ok");
  assert.equal(ask({}).status, "stale");
  assert.equal(ask({ expect: `${fingerprint}x` }).status, "stale");
  assert.equal(ask({ at: `${path}.9`, expect: fingerprint }).status, "stale");
  assert.equal(ask({ at: "", expect: fingerprint }).status, "stale");
});

test("the list is kept once, for one page, with only the place and fingerprint of each element, and a second list replaces it", async () => {
  const secret = el("input", { type: "text", "aria-label": "Full name" }, [], { held: "Ada Lovelace" });
  const fakeA = shop([secret, button("pay", "Pay now")]);
  const fakeB = shop([button("c", "Gamma"), button("d", "Delta")], OTHER);
  const chrome = browser({ fakes: { 1: fakeA, 2: fakeB } });
  const before = Date.now();
  await list(chrome);
  const first = stored(chrome);
  assert.deepEqual(Object.keys(first).sort(), ["at", "elements", "url"]);
  assert.equal(first.url, SHOP);
  assert.ok(first.at >= before && first.at <= Date.now());
  for (const element of first.elements) {
    assert.deepEqual(Object.keys(element).sort(), ["fingerprint", "path"]);
  }
  assert.ok(first.elements[0].fingerprint.startsWith("input|textbox|Full name|"));
  assert.ok(!JSON.stringify(first).includes("Ada Lovelace"));
  await list(chrome, OTHER);
  const second = stored(chrome);
  assert.equal(second.url, OTHER);
  assert.equal(second.elements.length, 2);
  assert.deepEqual(
    Object.keys(chrome.storage.session.data).filter((key) => key.startsWith(NUMBERS_KEY)),
    [NUMBERS_KEY],
  );
  assert.ok(!(NUMBERS_KEY in chrome.local), "the list is in storage that outlives the browser");
});

// The forgetting is done by the functions that turn a group off, as the
// keep_awake request is released.
function switching({ session, stored: local = {}, held = allOptional() } = {}) {
  const calls = [];
  const sessionData = { [NUMBERS_KEY]: { url: SHOP, at: Date.now(), elements: [{ path: "0", fingerprint: "button|button|A|0|" }] }, ...session };
  const localData = {
    [GROUP_SWITCHES_KEY]: { "page-read": true, input: true },
    [SETTINGS_KEY]: Object.fromEntries(NUMBERED.map((tool) => [tool, true])),
    ...local,
  };
  const chrome = {
    permissions: fakePermissions({ held, calls }),
    storage: { local: fakeArea(localData), session: fakeArea(sessionData) },
  };
  return { chrome, session: sessionData, stored: localData };
}

test("turning everything off forgets the list, and so does turning off either group that lists or uses numbers", async () => {
  const everything = switching();
  await turnEverythingOff(everything.chrome);
  assert.ok(!(NUMBERS_KEY in everything.session), "Turn everything off kept the list");

  for (const id of ["page-read", "input"]) {
    const one = switching();
    await turnOff(one.chrome, id);
    assert.ok(!(NUMBERS_KEY in one.session), `turning off ${id} kept the list`);
  }
});

test("a list left from before a group was turned off or its lifetime ended is not used when the group is on again", async () => {
  const lapsed = switching({
    stored: {
      [LIFETIMES_KEY]: {
        "page-read": { lifetime: "one-hour", expires: Date.now() - 1 },
        input: { lifetime: "one-hour", expires: Date.now() + 3_600_000 },
      },
    },
  });
  await turnOffLapsed(lapsed.chrome);
  assert.ok(!(NUMBERS_KEY in lapsed.session), "a list outlived its group's lifetime");

  const turnedOn = switching({ stored: { [GROUP_SWITCHES_KEY]: {} } });
  assert.equal(await turnOn(turnedOn.chrome, "input", { allowed: [], scopes: {} }), true);
  assert.ok(!(NUMBERS_KEY in turnedOn.session), "a list from before was kept");
});

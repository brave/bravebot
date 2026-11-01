// The input group: clicking, typing, choosing, pressing a key, scrolling and
// waiting in a page, by a role and an accessible name. The functions the
// extension injects run against a fake page (fake-dom.mjs), the confirmation
// window is a double that answers as a test says, and the browser is a fake
// that records every call.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { activityLog } from "../activity.js";
import {
  CONFIRMATION_WAIT_MS,
  approve,
  setConfirmation,
} from "../approval.js";
import { Refusal } from "../errors.js";
import { GROUPS, PRESETS, SCOPE_ALL } from "../groups.js";
import {
  FAILURES,
  OUTLINE_MS,
  WAIT_POLL_MS,
  clearOutlineInPage,
  inPage,
  pageReads,
  scrollInPage,
} from "../input.js";
import { RUNS_KEY, UNTIL_RESTART, defaultLifetime } from "../lifetimes.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  GROUP_SWITCHES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
  BUSY_MESSAGE,
  TOOLS,
  clearOutline,
  PAGE_WAIT_MS,
  handle,
  listInPage,
  performAction,
  readInPage,
  resolveElement,
} from "../tools.js";
import { KEYS, ROLES, SHOWN_CAP } from "../words.js";
import {
  HOSTILE as CONTROL,
  attributesOf,
  el,
  eventsOf,
  kindsOn,
  page,
  shadowed,
} from "./fake-dom.mjs";
import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

const SHOP = "https://shop.example/cart";
const ORIGIN = "https://shop.example";
const INPUT_TOOLS = GROUPS.input.tools;

// A fake browser with one tab open at SHOP whose page is `fake`, with the input
// group on, on all sites, and every permission granted unless the test says.
// `injected` keeps the options of every executeScript call. `stored` is added to
// what storage.local holds.
function browser({
  fake,
  tabs = [{ id: 1, windowId: 1, title: "Cart", url: SHOP }],
  granted = allOptional(),
  stored = {},
  failing,
  listing = false,
  hanging = false,
} = {}) {
  const calls = [];
  const injected = [];
  const groups = listing ? ["input", "page-read"] : ["input"];
  const local = {
    [SETTINGS_KEY]: Object.fromEntries(groups.flatMap((id) => GROUPS[id].tools).map((tool) => [tool, true])),
    [GROUP_SWITCHES_KEY]: Object.fromEntries(groups.map((id) => [id, true])),
    [SITE_SCOPES_KEY]: Object.fromEntries(groups.map((id) => [id, SCOPE_ALL])),
    ...stored,
  };
  const state = { hanging };
  return {
    calls,
    injected,
    local,
    state,
    permissions: fakePermissions({ held: granted, calls }),
    storage: {
      local: fakeArea(local),
      session: fakeArea({ [RUNS_KEY]: Object.fromEntries(groups.map((id) => [id, true])) }),
    },
    tabs: {
      async query(filter) {
        calls.push(["tabs.query", filter]);
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
        if (failing) {
          throw failing;
        }
        if (state.hanging) {
          return new Promise(() => {});
        }
        return [
          { frameId: 0, result: fake.run(() => options.func(...options.args)) },
        ];
      },
    },
  };
}

const call = (chrome, method, params) =>
  handle({ id: 1, method, params }, chrome);

const reached = (chrome, api) => chrome.calls.some(([name]) => name === api);

// The person's window: it records what it was shown and answers with a token for
// it, unless `answer` says otherwise. Until a test makes one, nobody allows
// anything.
function window(answer = approve) {
  const asked = [];
  setConfirmation(async (resolution) => {
    asked.push(resolution);
    return answer(resolution);
  });
  return asked;
}

afterEach(() => setConfirmation());

const shop = (body, more = {}) => page({ href: SHOP, body, ...more });

const button = (id, name, more = {}) =>
  el("button", { id }, [name], more);

// The page's events at the moment of the answer, without scrolling and focus.
const happenings = (fake) =>
  fake.events.filter((event) => !["scrollIntoView", "focus"].includes(event.type));

test("a click is found by role and name, shown to the person, and made only after they allow it", async () => {
  const fake = shop([
    el("h1", {}, ["Cart"]),
    button("pay", "Pay now"),
    el("a", { id: "home", href: "/" }, ["Home"], { href: "https://shop.example/" }),
  ]);
  const chrome = browser({ fake });
  let before;
  const asked = window((resolution) => {
    before = happenings(fake);
    return approve(resolution);
  });
  const reply = await call(chrome, "click_element", {
    url: SHOP,
    role: "button",
    name: "  pay   NOW ",
  });
  assert.deepEqual(reply.result, { status: "clicked", origin: ORIGIN });
  assert.equal(asked.length, 1);
  assert.deepEqual(asked[0].description, {
    action: "click",
    origin: ORIGIN,
    url: SHOP,
    tag: "button",
    role: "button",
    name: "Pay now",
    excerpt: "Pay now",
    goes: "",
    rect: { x: 10, y: 50, width: 100, height: 30 },
  });
  assert.deepEqual(before, [], "something happened before the person answered");
  assert.deepEqual(kindsOn(fake, fake.element("pay")), [
    "pointerdown",
    "mousedown",
    "pointerup",
    "mouseup",
    "click",
  ]);
  const click = eventsOf(fake, "click")[0].event;
  assert.equal(click.clientX, 60);
  assert.equal(click.clientY, 65);
  assert.equal(click.bubbles, true);
  assert.deepEqual(kindsOn(fake, fake.element("home")), []);
});

test("a double click is two clicks and a dblclick, and a right click is the context menu event alone", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  window();
  const base = { url: SHOP, role: "button", name: "Pay now" };
  await call(chrome, "click_element", { ...base, count: 2 });
  const twice = kindsOn(fake, fake.element("pay"));
  assert.deepEqual(twice.slice(-3), ["mouseup", "click", "dblclick"]);
  assert.equal(twice.filter((kind) => kind === "click").length, 2);
  assert.deepEqual(
    eventsOf(fake, "click").map((event) => event.event.detail),
    [1, 2],
  );
  fake.events.length = 0;
  const reply = await call(chrome, "click_element", { ...base, button: "right" });
  assert.deepEqual(reply.result, { status: "right clicked", origin: ORIGIN });
  assert.deepEqual(kindsOn(fake, fake.element("pay")), ["contextmenu"]);
  assert.equal(eventsOf(fake, "contextmenu")[0].event.button, 2);
});

test("a link names where it goes and a button in a form names the form's action, as text for the window", async () => {
  const form = el("form", {}, [], { action: "https://shop.example/charge" });
  const submit = button("buy", "Buy", { form });
  const fake = shop([
    el("a", { href: "/out" }, ["Leave"], { href: "https://elsewhere.example/" + "x".repeat(300) }),
    form,
    submit,
  ]);
  const chrome = browser({ fake });
  const asked = window(() => null);
  await call(chrome, "click_element", { url: SHOP, role: "link", name: "Leave" });
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Buy" });
  assert.equal(asked[0].description.goes, "https://elsewhere.example/" + "x".repeat(300));
  assert.equal(asked[1].description.goes, "https://shop.example/charge");
  for (const resolution of asked) {
    for (const value of Object.values(resolution.description)) {
      assert.ok(["string", "object"].includes(typeof value));
    }
  }
});

test("a long name, text and destination reach the window whole, up to a cap, so that the window is the one that cuts them", async () => {
  const long = "w".repeat(500);
  const whole = `https://elsewhere.example/${"x".repeat(1_500)}`;
  const fake = shop([
    el("a", { href: "/out", "aria-label": long }, [long], { href: whole }),
  ]);
  const chrome = browser({ fake });
  const found = await resolveElement(
    chrome,
    { url: SHOP },
    { action: "click", target: { role: "link", name: long } },
  );
  assert.equal(found.description.name, long);
  assert.equal(found.description.excerpt, long);
  assert.equal(found.description.goes, whole);
});

test("an element with a name, text or destination over the cap is refused before the person is asked, and nothing is cut to fit", async () => {
  const over = (fill) => fill.repeat(SHOWN_CAP + 1);
  // A name that long is reached only as the name of the element with the focus,
  // since a call names an element by at most 200 characters.
  const cases = {
    "a name": [
      el("button", { id: "pay", "aria-label": over("n") }, ["Pay"]),
      (chrome) => call(chrome, "press_key", { url: SHOP, key: "Enter" }),
    ],
    "a text": [
      el("button", { "aria-label": "Pay" }, [over("t")]),
      (chrome) => call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay" }),
    ],
    "a destination": [
      el("a", { href: "/", "aria-label": "Pay" }, ["Pay"], { href: `https://a.example/${over("d")}` }),
      (chrome) => call(chrome, "click_element", { url: SHOP, role: "link", name: "Pay" }),
    ],
  };
  for (const [why, [element, act]] of Object.entries(cases)) {
    const fake = shop([element]);
    fake.active = element;
    const chrome = browser({ fake });
    const asked = window();
    const reply = await act(chrome);
    assert.equal(reply.error?.message, FAILURES.long(), why);
    assert.equal(asked.length, 0, `${why} was put to the person`);
    assert.deepEqual(happenings(fake), [], `${why} was acted on`);
    assert.equal(fake.added().length, 0, `${why} was outlined`);
  }
  const edge = shop([el("button", { "aria-label": "Pay" }, ["t".repeat(SHOWN_CAP)])]);
  const asked = window();
  const reply = await call(browser({ fake: edge }), "click_element", {
    url: SHOP,
    role: "button",
    name: "Pay",
  });
  assert.equal(reply.result?.status, "clicked");
  assert.equal(asked[0].description.excerpt.length, SHOWN_CAP);
});

test("waiting for an element finds one whose words are longer than the cap, which it shows nobody", async () => {
  const fake = shop([el("button", { "aria-label": "Pay" }, ["t".repeat(SHOWN_CAP + 1)])]);
  const reply = await call(browser({ fake }), "wait_for_element", {
    url: SHOP,
    role: "button",
    name: "Pay",
    seconds: 1,
  });
  assert.deepEqual(reply.result, { status: "found", origin: ORIGIN });
});

// A name that matches none, or more than one, names nothing, and the page is
// never asked to guess: the call says how many matched and nothing about them.
test("a name that matches no element, or several, fails with how many matched and nothing of them, and the person is not asked", async () => {
  const fake = shop([
    button("a", "Pay now"),
    button("b", "Pay now"),
    button("c", "Pay later"),
  ]);
  const chrome = browser({ fake });
  const asked = window();
  const none = await call(chrome, "click_element", {
    url: SHOP,
    role: "button",
    name: "Checkout",
  });
  assert.equal(none.error.message, FAILURES.none());
  assert.match(none.error.message, /no element has that role and that name/);
  const several = await call(chrome, "click_element", {
    url: SHOP,
    role: "button",
    name: "Pay now",
  });
  assert.equal(
    several.error.message,
    "2 elements have that role and that name, so the name does not name one",
  );
  for (const reply of [none, several]) {
    assert.ok(!("result" in reply));
    assert.ok(!/Pay (now|later)/i.test(reply.error.message));
  }
  assert.equal(asked.length, 0);
  assert.deepEqual(happenings(fake), []);

  const other = await call(chrome, "click_element", {
    url: SHOP,
    role: "link",
    name: "Pay later",
  });
  assert.equal(other.error.message, FAILURES.none());
});

test("a hidden element is not counted, so a hidden decoy does not stop the visible one, and only a hidden one is said to be hidden", async () => {
  const decoy = button("decoy", "Pay now", { style: { display: "none" } });
  const fake = shop([decoy, button("real", "Pay now")]);
  const chrome = browser({ fake });
  window();
  const reply = await call(chrome, "click_element", {
    url: SHOP,
    role: "button",
    name: "Pay now",
  });
  assert.deepEqual(reply.result, { status: "clicked", origin: ORIGIN });
  assert.deepEqual(kindsOn(fake, decoy), []);
  assert.equal(eventsOf(fake, "click")[0].target, fake.element("real"));

  const hiders = {
    "display none": { style: { display: "none" } },
    "visibility hidden": { style: { visibility: "hidden" } },
    "visibility collapse": { style: { visibility: "collapse" } },
    "opacity 0": { style: { opacity: "0" } },
    "hidden attribute": { hidden: true },
    "no width": { box: { left: 10, top: 10, width: 0, height: 30 } },
    "no height": { box: { left: 10, top: 10, width: 100, height: 0 } },
    "one pixel": { box: { left: 10, top: 10, width: 1, height: 1 } },
  };
  for (const [why, more] of Object.entries(hiders)) {
    const lone = shop([button("only", "Pay now", more)]);
    const alone = browser({ fake: lone });
    const asked = window();
    const failed = await call(alone, "click_element", {
      url: SHOP,
      role: "button",
      name: "Pay now",
    });
    assert.equal(failed.error?.message, FAILURES.hidden(), why);
    assert.equal(asked.length, 0, why);
    assert.deepEqual(happenings(lone), [], why);
  }

  const wrapped = {
    "a hidden ancestor": el("div", { hidden: "" }, [button("only", "Pay now")], { hidden: true }),
    "an ancestor not displayed": el("div", {}, [button("only", "Pay now")], { style: { display: "none" } }),
    "a transparent ancestor": el("div", {}, [button("only", "Pay now")], { style: { opacity: "0" } }),
    "an aria-hidden ancestor": el("div", { "aria-hidden": "true" }, [button("only", "Pay now")]),
    "aria-hidden itself": button("only", "Pay now"),
  };
  wrapped["aria-hidden itself"].attributes["aria-hidden"] = "true";
  for (const [why, body] of Object.entries(wrapped)) {
    const lone = shop([body]);
    const failed = await call(browser({ fake: lone }), "click_element", {
      url: SHOP,
      role: "button",
      name: "Pay now",
    });
    assert.equal(failed.error?.message, FAILURES.hidden(), why);
  }
});

// The name is the accessible name in the parts that matter, taken from the first
// of these that the element has.
test("the accessible name is taken from aria-labelledby, then aria-label, then the labels, the value, the alt text, the content and the title and placeholder", async () => {
  const table = [
    ["aria-label", el("button", { "aria-label": "  Close   dialog " }, ["X"]), "button", "Close dialog"],
    [
      "aria-labelledby, over aria-label and the content",
      [
        el("span", { id: "t" }, ["Billing ", el("b", {}, ["address"])]),
        el("input", { type: "text", "aria-labelledby": "t", "aria-label": "Other" }),
      ],
      "textbox",
      "Billing address",
    ],
    [
      "a label with for, over the placeholder",
      [
        el("label", { for: "email" }, ["Email address"]),
        el("input", { id: "email", type: "email", placeholder: "you@example" }),
      ],
      "textbox",
      "Email address",
    ],
    [
      "a wrapping label",
      el("label", {}, ["Phone ", el("input", { type: "tel" })]),
      "textbox",
      "Phone",
    ],
    ["a submit button's value", el("input", { type: "submit", value: "Search" }), "button", "Search"],
    ["an image button's alt", el("input", { type: "image", alt: "Go" }), "button", "Go"],
    [
      "an image inside a link",
      el("a", { href: "/" }, [el("img", { alt: "Home page" })]),
      "link",
      "Home page",
    ],
    [
      "block children with a space between",
      el("button", {}, [el("div", {}, ["Add"]), el("div", {}, ["to cart"])]),
      "button",
      "Add to cart",
    ],
    [
      "inline children without one",
      el("button", {}, [el("span", {}, ["Check"]), el("span", {}, ["out"])]),
      "button",
      "Checkout",
    ],
    [
      "content without what is hidden in it",
      el("button", {}, ["Pay", el("span", {}, ["secret"], { style: { display: "none" } }), el("i", { "aria-hidden": "true" }, ["x"])]),
      "button",
      "Pay",
    ],
    ["a title when nothing else names it", el("button", { title: "Help" }), "button", "Help"],
    ["a placeholder last", el("input", { type: "search", placeholder: "Search the shop" }), "textbox", "Search the shop"],
    ["an option's label", el("option", { label: "Savings" }, ["savings account"]), "option", "Savings"],
  ];
  for (const [why, body, role, name] of table) {
    const fake = shop(Array.isArray(body) ? body : [body]);
    const chrome = browser({ fake });
    const found = await resolveElement(
      chrome,
      { url: SHOP },
      { action: "click", target: { role, name } },
    );
    assert.equal(found.description.name, name, why);
  }
});

test("an element has the role its tag or role attribute gives it, from the list, and no other", async () => {
  const table = [
    ["a link", el("a", { href: "/" }, ["L"]), "link"],
    ["a button", el("button", {}, ["L"]), "button"],
    ["an input of type button", el("input", { type: "button", value: "L" }), "button"],
    ["a text input", el("input", { "aria-label": "L" }), "textbox"],
    ["a search input", el("input", { type: "search", "aria-label": "L" }), "textbox"],
    ["an input with a list", el("input", { list: "x", "aria-label": "L" }), "combobox"],
    ["a text area", el("textarea", { "aria-label": "L" }), "textbox"],
    ["a checkbox", el("input", { type: "checkbox", "aria-label": "L" }), "checkbox"],
    ["a radio", el("input", { type: "radio", "aria-label": "L" }), "radio"],
    ["a select", el("select", { "aria-label": "L" }), "combobox"],
    ["an option", el("option", {}, ["L"]), "option"],
    ["an editing host", el("div", { contenteditable: "true", "aria-label": "L" }), "textbox"],
    ["role=menuitem", el("div", { role: "menuitem" }, ["L"]), "menuitem"],
    ["role=tab", el("div", { role: "tab" }, ["L"]), "tab"],
    ["role=switch", el("div", { role: "switch", "aria-label": "L" }), "switch"],
    ["role over the tag", el("a", { href: "/", role: "button" }, ["L"]), "button"],
    ["the first role of a list it has", el("div", { role: "presentation tab" }, ["L"]), "tab"],
  ];
  for (const [why, element, role] of table) {
    const chrome = browser({ fake: shop([element]) });
    const found = await resolveElement(
      chrome,
      { url: SHOP },
      { action: "click", target: { role, name: "L" } },
    );
    assert.equal(found.description.role, role, why);
    // Not found as any other role.
    for (const other of ROLES.filter((made) => made !== role)) {
      const failed = await resolveElement(
        browser({ fake: shop([element]) }),
        { url: SHOP },
        { action: "click", target: { role: other, name: "L" } },
      ).catch((error) => error);
      assert.ok(failed instanceof Error, `${why} was found as ${other}`);
    }
  }
  const none = [
    ["an anchor with no href", el("a", {}, ["L"])],
    ["a number input", el("input", { type: "number", "aria-label": "L" })],
    ["a select of many", el("select", { "aria-label": "L" }, [], { multiple: true })],
    ["a hidden input", el("input", { type: "hidden", "aria-label": "L" })],
    ["a div", el("div", {}, ["L"])],
    ["an unlisted role", el("div", { role: "slider", "aria-label": "L" })],
  ];
  for (const [why, element] of none) {
    for (const role of ROLES) {
      const failed = await resolveElement(
        browser({ fake: shop([element]) }),
        { url: SHOP },
        { action: "click", target: { role, name: "L" } },
      ).catch((error) => error);
      assert.ok(failed instanceof Error, `${why} was found as ${role}`);
    }
  }
});

test("an element in an open or a closed shadow root is found, and a frame is not entered", async () => {
  const closed = shadowed(el("div"), "closed", [button("deep", "Deep")]);
  const open = shadowed(el("div"), "open", [button("shallow", "Shallow")]);
  const frame = el("iframe", { id: "frame" });
  const entered = [];
  for (const member of ["contentDocument", "contentWindow"]) {
    Object.defineProperty(frame, member, {
      get() {
        entered.push(member);
        return null;
      },
    });
  }
  const fake = shop([closed, open, frame]);
  window();
  const chrome = browser({ fake });
  for (const [name, id] of [["Deep", "deep"], ["Shallow", "shallow"]]) {
    fake.events.length = 0;
    const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name });
    assert.equal(reply.result?.status, "clicked", name);
    assert.equal(eventsOf(fake, "click")[0].target, fake.element(id));
  }
  const inside = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Inside" });
  assert.match(inside.error.message, /frames are not searched/);
  assert.deepEqual(entered, [], "a frame was entered");

  // A browser with no chrome.dom still enters an open root and not a closed one.
  const without = browser({ fake });
  const noDom = {
    ...without,
    scripting: {
      async executeScript(options) {
        return [{ result: fake.run(() => options.func(...options.args), { dom: false }) }];
      },
    },
  };
  const shallow = await call(noDom, "wait_for_element", { url: SHOP, role: "button", name: "Shallow", seconds: 1 });
  assert.equal(shallow.result?.status, "found");
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const waiting = call(noDom, "wait_for_element", { url: SHOP, role: "button", name: "Deep", seconds: 1 });
    for (let at = 0; at < 1000; at += WAIT_POLL_MS) {
      await settled();
      mock.timers.tick(WAIT_POLL_MS);
    }
    await settled();
    mock.timers.tick(1);
    assert.match((await waiting).error.message, /no single element/);
  } finally {
    mock.timers.reset();
  }
});

// Lets the calls a test started finish what they were waiting for.
async function settled() {
  for (let turn = 0; turn < 20; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
}

const field = (type, attributes = {}, more = {}) =>
  el("input", { type, "aria-label": "Field", ...attributes }, [], more);

test("a password field, a payment card field and a file chooser are never acted on, and the person is not asked", async () => {
  const fields = {
    password: field("password"),
    "a text field that says it holds a password": field("text", { autocomplete: "current-password" }),
    "a text field for a new password": field("text", { autocomplete: "new-password" }),
    "card number": field("text", { autocomplete: "cc-number" }),
    "card code among other words": field("text", { autocomplete: "section-pay billing cc-csc" }),
    "card expiry, upper case": field("text", { autocomplete: "CC-EXP" }),
    "file chooser": field("file"),
  };
  const kinds = {
    password: /password field/,
    "a text field that says it holds a password": /password field/,
    "a text field for a new password": /password field/,
    "card number": /payment card field/,
    "card code among other words": /payment card field/,
    "card expiry, upper case": /payment card field/,
    "file chooser": /file chooser/,
  };
  for (const [why, element] of Object.entries(fields)) {
    for (const [method, more] of [
      ["click_element", {}],
      ["type_text", { text: "x" }],
      ["select_option", { option: "x" }],
    ]) {
      const fake = shop([element]);
      const chrome = browser({ fake });
      const asked = window();
      const role = element.attributes.type === "file" ? "button" : "textbox";
      const reply = await call(chrome, method, { url: SHOP, role, name: "Field", ...more });
      assert.match(reply.error?.message ?? "", kinds[why], `${method} on ${why}`);
      assert.match(reply.error.message, /never acted on/);
      assert.equal(asked.length, 0, `${method} on ${why} asked the person`);
      assert.deepEqual(happenings(fake), [], `${method} on ${why} did something`);
      const [entry] = await activityLog(chrome);
      assert.equal(entry.outcome, "refused", `${method} on ${why}`);
    }
  }
  // The same fields are refused for a key, as the element that has the focus.
  for (const [why, element] of Object.entries(fields)) {
    const fake = shop([element]);
    fake.active = element;
    const chrome = browser({ fake });
    const asked = window();
    const reply = await call(chrome, "press_key", { url: SHOP, key: "Enter" });
    assert.match(reply.error?.message ?? "", kinds[why], `a key on ${why}`);
    assert.equal(asked.length, 0);
    assert.deepEqual(happenings(fake), []);
  }
});

test("a disabled or read-only element is not acted on and is failed, not refused", async () => {
  const fieldset = el("fieldset", {}, [field("text")], { disabled: true });
  const cases = {
    disabled: button("b", "Field", { disabled: true }),
    "read only": field("text", {}, { readOnly: true }),
    "aria-disabled": button("b", "Field"),
    "in a disabled fieldset": fieldset,
  };
  cases["aria-disabled"].attributes["aria-disabled"] = "true";
  for (const [why, element] of Object.entries(cases)) {
    const fake = shop([element]);
    const chrome = browser({ fake });
    const asked = window();
    const role = element.localName === "button" ? "button" : "textbox";
    const reply = await call(chrome, "click_element", { url: SHOP, role, name: "Field" });
    assert.equal(reply.error?.message, FAILURES.disabled(), why);
    assert.equal(asked.length, 0, why);
    assert.deepEqual(happenings(fake), [], why);
    assert.equal((await activityLog(chrome))[0].outcome, "failed", why);
  }
});

// The window can be answered a long time after the element was found, so the
// page is asked again at the moment of the action.
test("an element covered by another at its middle is reported as covered when the action is made, and nothing is done", async () => {
  const veil = el("div", { id: "veil" }, [], {
    box: { left: 0, top: 0, width: 500, height: 500 },
    style: { display: "none" },
  });
  const fake = shop([button("pay", "Pay now"), veil]);
  const chrome = browser({ fake });
  const asked = window((resolution) => {
    veil.style.display = "block";
    return approve(resolution);
  });
  const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(reply.error?.message, FAILURES.covered());
  assert.equal(asked.length, 1, "the person was asked before the cover came");
  assert.deepEqual(happenings(fake), []);
  assert.equal((await activityLog(chrome))[0].outcome, "failed");

  // A frame over it covers it too.
  const framed = shop([
    button("pay", "Pay now"),
    el("iframe", { id: "ad" }, [], { box: { left: 0, top: 0, width: 500, height: 500 } }),
  ]);
  window();
  const other = await call(browser({ fake: framed }), "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(other.error?.message, FAILURES.covered());
  assert.deepEqual(happenings(framed), []);
});

test("an element at the middle of its own label, or of what it holds, is not covered, and one outside the window's view is not clicked", async () => {
  const input = el("input", { id: "c", type: "checkbox", "aria-label": "Agree" }, [], {
    box: { left: 10, top: 10, width: 30, height: 30 },
  });
  const label = el("label", { for: "c" }, ["Agree"], {
    box: { left: 10, top: 10, width: 30, height: 30 },
  });
  const fake = shop([input, label]);
  window();
  const labelled = await call(browser({ fake }), "click_element", { url: SHOP, role: "checkbox", name: "Agree" });
  assert.equal(labelled.result?.status, "clicked");
  assert.deepEqual(kindsOn(fake, input), ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]);

  const holding = el("button", { id: "h" }, [el("span", { id: "inner" }, ["Pay"])], {
    box: { left: 10, top: 10, width: 100, height: 30 },
  });
  holding.children[0].box = { left: 10, top: 10, width: 100, height: 30 };
  const fakeHolding = shop([holding]);
  const held = await call(browser({ fake: fakeHolding }), "click_element", { url: SHOP, role: "button", name: "Pay" });
  assert.equal(held.result?.status, "clicked");

  const far = shop([button("pay", "Pay now", { box: { left: 5000, top: 10, width: 100, height: 30 } })]);
  const away = await call(browser({ fake: far }), "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(away.error?.message, FAILURES.offscreen());
  assert.deepEqual(happenings(far), []);
});

test("an element that is not the one the person was shown is not acted on: a swapped one, a second one, a renamed one and a page that left", async () => {
  const swaps = {
    "the shown element hidden and a hidden twin shown": (fake) => {
      fake.element("shown").style.display = "none";
      fake.element("twin").style.display = "block";
      return /changed after it was found/;
    },
    "a second element with the name shown": (fake) => {
      fake.element("twin").style.display = "block";
      return /2 elements have that role/;
    },
    "the shown element renamed": (fake) => {
      fake.element("shown").attributes["aria-label"] = "Pay later";
      return /only element with that role and that name is hidden/;
    },
    "the page left the URL": (fake) => {
      fake.href = "https://elsewhere.example/";
      return /left the URL/;
    },
  };
  for (const [why, change] of Object.entries(swaps)) {
    const fake = shop([
      button("shown", "Pay now"),
      button("twin", "Pay now", { style: { display: "none" } }),
    ]);
    const chrome = browser({ fake });
    let expected;
    const asked = window((resolution) => {
      expected = change(fake);
      return approve(resolution);
    });
    const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.match(reply.error?.message ?? "", expected, why);
    assert.equal(asked.length, 1, why);
    assert.deepEqual(happenings(fake), [], `${why} did something`);
  }
});

test("an element that moved in the document, or changed its tag, is not the one that was shown", async () => {
  const cases = {
    "moved to another place": (fake) => {
      const shown = fake.element("shown");
      const parent = shown.parentNode;
      parent.childNodes.reverse();
    },
    "replaced by an element of another tag": (fake) => {
      const shown = fake.element("shown");
      const parent = shown.parentNode;
      const made = el("a", { id: "shown", href: "/", role: "button" }, ["Pay now"], { box: shown.box });
      made.parentNode = parent;
      parent.childNodes[parent.childNodes.indexOf(shown)] = made;
    },
  };
  for (const [why, change] of Object.entries(cases)) {
    const fake = shop([el("div", {}, [button("shown", "Pay now"), el("p", {}, ["x"])])]);
    const chrome = browser({ fake });
    window((resolution) => {
      change(fake);
      return approve(resolution);
    });
    const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.match(reply.error?.message ?? "", /changed after it was found/, why);
    assert.deepEqual(happenings(fake), [], why);
  }
});

test("the function that acts does nothing without a token made for its own resolution, and a token is used once", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  const spec = { action: "click", target: { role: "button", name: "Pay now" } };
  const params = { url: SHOP };
  const resolution = await resolveElement(chrome, params, spec);
  const another = await resolveElement(chrome, params, spec);
  const found = chrome.injected.length;

  for (const [why, token] of [
    ["none", undefined],
    ["null", null],
    ["true", true],
    ["a made-up object", Object.freeze({})],
    ["a token for another resolution", approve(another)],
    ["the resolution itself", resolution],
  ]) {
    await assert.rejects(performAction(chrome, resolution, token), /not allowed in the browser/, why);
  }
  assert.equal(chrome.injected.length, found, "the page was reached without a token");
  assert.deepEqual(happenings(fake), []);

  const token = approve(resolution);
  assert.deepEqual(await performAction(chrome, resolution, token), { status: "clicked", origin: ORIGIN });
  assert.equal(eventsOf(fake, "click").length, 1);
  await assert.rejects(performAction(chrome, resolution, token), /not allowed in the browser/);
  assert.equal(eventsOf(fake, "click").length, 1, "a token served twice");
});

test("until a window is set, and when the person denies or closes it, nothing is done", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const params = { url: SHOP, role: "button", name: "Pay now" };
  const unset = await call(browser({ fake }), "click_element", params);
  assert.match(unset.error.message, /not allowed in the browser/);
  const asked = window(() => null);
  const denied = browser({ fake });
  const reply = await call(denied, "click_element", params);
  assert.match(reply.error.message, /not allowed in the browser/);
  assert.equal(asked.length, 1);
  assert.deepEqual(happenings(fake), []);
  assert.equal((await activityLog(denied))[0].outcome, "refused");
});

// The outline is a hint for the person and not a thing the approval depends on:
// it is an element of ours in a closed shadow root, with no word of the page, that
// takes no pointer event, and it goes whichever way the question ends.
test("the element is brought into view and outlined in a closed shadow root that holds no word of the page and takes no click", async () => {
  const fake = shop([button("pay", "Pay <b>now</b>")]);
  const chrome = browser({ fake });
  let during;
  window((resolution) => {
    during = {
      added: fake.added(),
      shadows: [...fake.shadows],
      scrolled: eventsOf(fake, "scrollIntoView").map((event) => [event.target, event.options]),
      tabs: chrome.calls.filter(([name]) => name === "tabs.update"),
    };
    return approve(resolution);
  });
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay <b>now</b>" });
  assert.equal(during.added.length, 1, "one element of ours in the page");
  const [host] = during.added;
  assert.equal(host.localName, "div");
  assert.deepEqual(during.shadows, [{ host, mode: "closed" }]);
  assert.equal(host.shadowRoot, null, "the page can reach the outline");
  assert.equal(host.textContent, "");
  assert.equal(host.shadow.children.length, 1);
  const frame = host.shadow.children[0];
  assert.equal(frame.textContent, "", "a word of the page is in the outline");
  assert.deepEqual(frame.attributes, {});
  assert.match(host.style.cssText, /pointer-events: none !important/);
  assert.match(host.style.cssText, /z-index: 2147483647 !important/);
  assert.match(frame.style.cssText, /pointer-events: none/);
  assert.match(frame.style.cssText, /left: 6px; top: 6px; width: 108px; height: 38px/);
  assert.deepEqual(during.scrolled, [
    [fake.element("pay"), { block: "center", inline: "center", behavior: "instant" }],
  ]);
  assert.deepEqual(during.tabs, [["tabs.update", 1, { active: true }]]);
  assert.deepEqual(fake.risky, []);
});

test("the outline is placed where the element is on the page, scrolled or not", async () => {
  const fake = shop([button("pay", "Pay now", { box: { left: 30, top: 70, width: 50, height: 20 } })]);
  fake.scrollX = 15;
  fake.scrollY = 400;
  const chrome = browser({ fake });
  let frame;
  window((resolution) => {
    frame = fake.added()[0].shadow.children[0];
    return approve(resolution);
  });
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.match(frame.style.cssText, /left: 41px; top: 466px; width: 58px; height: 28px/);
});

test("the outline is taken out of the page when the person allows, denies, closes the window or the window fails, and before the click", async () => {
  const ways = {
    allow: approve,
    deny: () => null,
    refused: () => {
      throw new Refusal("you denied the action in the browser");
    },
    "a window that fails": () => {
      throw new Error("the window broke");
    },
  };
  for (const [way, answer] of Object.entries(ways)) {
    const fake = shop([button("pay", "Pay now")]);
    const chrome = browser({ fake });
    let present;
    window((resolution) => {
      present = fake.added().length;
      return answer(resolution);
    });
    const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.equal(present, 1, way);
    assert.deepEqual(fake.added(), [], `${way} left the outline in the page`);
    assert.equal(fake.removed.length, 1, way);
    assert.equal(reply.result?.status === "clicked", way === "allow", way);
    const funcs = chrome.injected.map((made) => made.func);
    assert.deepEqual(
      funcs,
      way === "allow"
        ? [inPage, clearOutlineInPage, inPage]
        : [inPage, clearOutlineInPage],
      `${way}: the outline is cleared before the click and after nothing else`,
    );
  }
});

test("the tab not coming forward is a failure that clears the outline and does not ask", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  chrome.tabs.update = async () => {
    throw new Error("No tab with id: 1.");
  };
  const asked = window();
  const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.match(reply.error.message, /could not be brought forward to show the element: No tab with id/);
  assert.equal(asked.length, 0);
  assert.deepEqual(fake.added(), []);
  const next = await call(browser({ fake }), "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(next.result?.status, "clicked", "a failed action left the next one refused");
});

test("the outline hides itself after the window's time, a second one replaces the first, and a page that removed it does not stop the action", async () => {
  const fake = shop([button("pay", "Pay now"), button("other", "Other")]);
  const chrome = browser({ fake });
  const spec = { action: "click", target: { role: "button", name: "Pay now" }, outline: true };
  await resolveElement(chrome, { url: SHOP }, spec);
  const again = await resolveElement(chrome, { url: SHOP }, spec);
  assert.equal(fake.added().length, 1, "two outlines");
  assert.equal(fake.animations.length, 2);
  for (const made of fake.animations) {
    assert.equal(made.target.getRootNode().mode, "closed", "the box that fades is not inside the root");
    assert.deepEqual(made.frames, [{ opacity: 1 }, { opacity: 0 }]);
    assert.deepEqual(made.options, { delay: OUTLINE_MS, duration: 1, fill: "forwards" });
  }
  assert.ok(OUTLINE_MS > CONFIRMATION_WAIT_MS);

  // The page removes the outline while the person decides.
  fake.run(() => fake.added()[0].remove());
  window();
  const reply = await call(browser({ fake }), "click_element", { url: SHOP, role: "button", name: "Other" });
  assert.equal(reply.result?.status, "clicked");
  await clearOutline(chrome, again);
  assert.deepEqual(fake.added(), []);
});

test("taking the outline away never fails, whatever happened to the page", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  const found = await resolveElement(chrome, { url: SHOP }, { action: "click", target: { role: "button", name: "Pay now" }, outline: true });
  const gone = browser({ fake, failing: new Error("No tab with id: 1.") });
  await clearOutline(gone, found);
  const bare = browser({ fake });
  bare.scripting.executeScript = async () => [];
  await clearOutline(bare, found);
});

test("a key is outlined on the element that has the focus, and a page with nothing focused gets no outline", async () => {
  const fake = shop([field("text", { id: "q", "aria-label": "Search" })]);
  const chrome = browser({ fake });
  fake.active = fake.element("q");
  let present;
  window((resolution) => {
    present = fake.added().length;
    return approve(resolution);
  });
  await call(chrome, "press_key", { url: SHOP, key: "Enter" });
  assert.equal(present, 1);
  const bare = shop([el("p", {}, ["text"])]);
  const bareChrome = browser({ fake: bare });
  window((resolution) => {
    present = bare.added().length;
    return approve(resolution);
  });
  await call(bareChrome, "press_key", { url: SHOP, key: "Tab" });
  assert.equal(present, 0, "the body has no size to outline");
});

test("only one action waits for the person at a time: a second is refused, reaches no page, and the next one after the first is answered", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const asked = window(async (resolution) => {
    await held;
    return approve(resolution);
  });
  const params = { url: SHOP, role: "button", name: "Pay now" };
  const first = call(chrome, "click_element", params);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(asked.length, 1);
  const reached = chrome.injected.length;
  const second = await call(chrome, "click_element", params);
  assert.equal(second.error.message, BUSY_MESSAGE);
  assert.match(second.error.message, /another action is waiting for your answer/);
  assert.equal(chrome.injected.length, reached, "the second reached the page");
  assert.equal(asked.length, 1);
  assert.equal(fake.added().length, 1, "the first's outline was disturbed");
  const press = await call(chrome, "press_key", { url: SHOP, key: "Tab" });
  assert.equal(press.error.message, BUSY_MESSAGE);
  release();
  assert.equal((await first).result.status, "clicked");
  assert.equal(
    (await activityLog(chrome)).filter((entry) => entry.outcome === "refused").length,
    2,
  );
  const third = await call(chrome, "click_element", params);
  assert.equal(third.result.status, "clicked");
  // A question that fails does not leave the next one refused.
  window(() => {
    throw new Error("the window broke");
  });
  await call(chrome, "click_element", params);
  window();
  const last = await call(chrome, "click_element", params);
  assert.notEqual(last.error?.message, BUSY_MESSAGE);
});

test("where a link goes, or where a button's form is sent, is part of what was shown, and a change after the window is not acted on", async () => {
  const changes = {
    "a link sent somewhere else": (fake) => {
      fake.element("go").href = "https://evil.example/";
    },
    "a button given a form action of its own": (fake) => {
      const submit = fake.element("buy");
      submit.attributes.formaction = "https://evil.example/pay";
      submit.formAction = "https://evil.example/pay";
    },
    "a form sent somewhere else": (fake) => {
      fake.element("buy").form.action = "https://evil.example/pay";
    },
  };
  for (const [why, change] of Object.entries(changes)) {
    const form = el("form", {}, [], { action: "https://shop.example/charge" });
    const fake = shop([
      el("a", { id: "go", href: "/ok" }, ["Go"], { href: "https://shop.example/ok" }),
      form,
      button("buy", "Buy", { form }),
    ]);
    const chrome = browser({ fake });
    window((resolution) => {
      change(fake);
      return approve(resolution);
    });
    const target = why.startsWith("a link") ? ["link", "Go"] : ["button", "Buy"];
    const reply = await call(chrome, "click_element", { url: SHOP, role: target[0], name: target[1] });
    assert.match(reply.error?.message ?? "", /changed after it was found/, why);
    assert.deepEqual(happenings(fake), [], `${why} was acted on`);
  }
});

test("a button with a form action of its own shows that action, and not its form's", async () => {
  const form = el("form", {}, [], { action: "https://shop.example/cancel" });
  const submit = button("buy", "Cancel", {
    form,
    formAction: "https://evil.example/pay",
  });
  submit.attributes.formaction = "https://evil.example/pay";
  const fake = shop([form, submit]);
  const chrome = browser({ fake });
  const asked = window(() => null);
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Cancel" });
  assert.equal(asked[0].description.goes, "https://evil.example/pay");
});

test("the address the window shows is the tab's, whole up to the cap, and a longer one is refused before the page is reached", async () => {
  const long = `${SHOP}?q=${"x".repeat(400)}`;
  const fake = page({ href: long, body: [button("pay", "Pay now")] });
  const chrome = browser({ fake, tabs: [{ id: 1, windowId: 1, title: "Cart", url: long }] });
  const asked = window(() => null);
  await call(chrome, "click_element", { url: long, role: "button", name: "Pay now" });
  assert.equal(asked[0].description.url, long);
  assert.equal(asked[0].description.origin, ORIGIN);
  const huge = `${SHOP}?q=${"x".repeat(SHOWN_CAP)}`;
  const away = browser({
    fake: page({ href: huge, body: [button("pay", "Pay now")] }),
    tabs: [{ id: 1, windowId: 1, title: "Cart", url: huge }],
  });
  const refused = await call(away, "click_element", { url: huge, role: "button", name: "Pay now" });
  assert.match(refused.error?.message ?? "", /longer than 2000 characters/);
  assert.ok(!reached(away, "scripting.executeScript"));
  assert.equal(asked.length, 1);
});

test("typing puts the text in the field as text, in place of what it held, and tells the page the way a person typing does", async () => {
  const input = field("text", { "aria-label": "Amount" }, { held: "old" });
  const fake = shop([input]);
  const chrome = browser({ fake });
  const asked = window();
  const reply = await call(chrome, "type_text", {
    url: SHOP,
    role: "textbox",
    name: "Amount",
    text: "42.00",
  });
  assert.deepEqual(reply.result, { status: "typed", origin: ORIGIN });
  assert.equal(input.value, "42.00");
  assert.deepEqual(kindsOn(fake, input), ["value", "input", "change"]);
  const typed = eventsOf(fake, "input")[0].event;
  assert.equal(typed.inputType, "insertText");
  assert.equal(typed.data, "42.00");
  assert.equal(typed.bubbles, true);
  assert.equal(asked[0].description.text, "42.00");
  assert.equal(asked[0].description.action, "type");
  assert.ok(eventsOf(fake, "focus").length > 0);

  const area = shop([el("textarea", { "aria-label": "Note" }, [], { held: "" })]);
  const note = await call(browser({ fake: area }), "type_text", { url: SHOP, role: "textbox", name: "Note", text: "a\nb" });
  assert.equal(note.result?.status, "typed");
});

test("typing in an editing host inserts the text with the editing command, and fails where the page does not take it", async () => {
  const host = el("div", { contenteditable: "true", "aria-label": "Message" }, ["old"]);
  const fake = shop([host]);
  const chrome = browser({ fake });
  window();
  const reply = await call(chrome, "type_text", { url: SHOP, role: "textbox", name: "Message", text: "hello" });
  assert.equal(reply.result?.status, "typed");
  assert.deepEqual(
    fake.commands,
    [
      { name: "selectAll", value: undefined },
      { name: "insertText", value: "hello" },
    ],
  );
  fake.editing = false;
  const refused = await call(chrome, "type_text", { url: SHOP, role: "textbox", name: "Message", text: "hello" });
  assert.equal(refused.error?.message, FAILURES.unsupported());
});

test("typing in an element that is not a text field fails, and so does choosing in one that is not a native select", async () => {
  const fake = shop([
    el("div", { role: "textbox", "aria-label": "Fake" }),
    button("b", "Press"),
    el("div", { role: "combobox", "aria-label": "Custom" }),
  ]);
  const chrome = browser({ fake });
  window();
  const typed = await call(chrome, "type_text", { url: SHOP, role: "textbox", name: "Fake", text: "x" });
  assert.equal(typed.error?.message, FAILURES.unsupported());
  const chosen = await call(chrome, "select_option", { url: SHOP, role: "combobox", name: "Custom", option: "x" });
  assert.equal(chosen.error?.message, FAILURES.unsupported());
  assert.deepEqual(happenings(fake), []);
});

test("choosing picks the one option with the text, and tells the page; no option, two options or a disabled option fail", async () => {
  const options = [
    el("option", { value: "c" }, ["Checking"]),
    el("option", { value: "s" }, ["Savings"]),
    el("option", { value: "t" }, ["Twin"]),
    el("option", { value: "u" }, ["twin"]),
    el("option", { value: "d" }, ["Closed"], { disabled: true }),
  ];
  const select = el("select", { id: "acct", "aria-label": "Account" }, options);
  const fake = shop([select]);
  const chrome = browser({ fake });
  const asked = window();
  const base = { url: SHOP, role: "combobox", name: "Account" };
  const reply = await call(chrome, "select_option", { ...base, option: "savings" });
  assert.deepEqual(reply.result, { status: "selected", origin: ORIGIN });
  assert.equal(options[1].selected, true);
  assert.equal(options[0].selected, false);
  assert.deepEqual(
    fake.events.filter((event) => event.target === options[1]).map((event) => event.type),
    ["selected"],
  );
  assert.deepEqual(kindsOn(fake, select), ["input", "change"]);
  assert.equal(asked[0].description.option, "savings");
  for (const [option, message] of [
    ["Nothing", FAILURES.no_option()],
    ["twin", FAILURES.several_options(2)],
    ["Closed", FAILURES.disabled()],
  ]) {
    fake.events.length = 0;
    const failed = await call(chrome, "select_option", { ...base, option });
    assert.equal(failed.error?.message, message, option);
    assert.deepEqual(happenings(fake), [], option);
  }
});

test("a key is sent to the element that has the focus, which the person is shown, as a key down and up", async () => {
  const search = field("search", { "aria-label": "Search" });
  const fake = shop([search, button("other", "Other")]);
  fake.active = search;
  const chrome = browser({ fake });
  const asked = window();
  const reply = await call(chrome, "press_key", { url: SHOP, key: "Enter" });
  assert.deepEqual(reply.result, { status: "pressed", origin: ORIGIN });
  assert.equal(asked[0].description.tag, "input");
  assert.equal(asked[0].description.role, "textbox");
  assert.equal(asked[0].description.name, "Search");
  assert.equal(asked[0].description.key, "Enter");
  assert.deepEqual(kindsOn(fake, search), ["keydown", "keypress", "keyup"]);
  const down = eventsOf(fake, "keydown")[0].event;
  assert.equal(down.key, "Enter");
  assert.equal(down.keyCode, 13);
  assert.equal(down.ctrlKey, undefined);
  assert.deepEqual(kindsOn(fake, fake.element("other")), []);

  fake.events.length = 0;
  await call(chrome, "press_key", { url: SHOP, key: "ArrowDown" });
  assert.deepEqual(kindsOn(fake, search), ["keydown", "keyup"]);

  // With nothing focused the key goes to the page's body.
  const bare = shop([button("other", "Other")]);
  const noFocus = window();
  const sent = await call(browser({ fake: bare }), "press_key", { url: SHOP, key: "PageDown" });
  assert.equal(sent.result?.status, "pressed");
  assert.equal(noFocus[0].description.tag, "body");
  assert.equal(noFocus[0].description.role, null);
});

test("a key the extension does not send is refused before the page is reached", async () => {
  const fake = shop([button("other", "Other")]);
  const chrome = browser({ fake });
  const asked = window();
  for (const key of ["Control+a", "a", "enter", "F5", "Enter ", "", "Meta", "Shift+Tab", undefined, 13, ["Enter"]]) {
    const reply = await call(chrome, "press_key", { url: SHOP, key });
    assert.equal(reply.error?.code, -32602, JSON.stringify(key));
  }
  assert.equal(asked.length, 0);
  assert.ok(!reached(chrome, "scripting.executeScript"));
  assert.ok(!reached(chrome, "tabs.query"));
  for (const key of KEYS) {
    const sent = await call(chrome, "press_key", { url: SHOP, key });
    assert.equal(sent.result?.status, "pressed", key);
  }
});

test("scrolling moves the page by screens, or to its top or bottom, and the person is not asked", async () => {
  const html = null;
  const fake = shop([button("pay", "Pay now")]);
  fake.document.documentElement.scrollHeight = 9000;
  const chrome = browser({ fake });
  const asked = window();
  const base = { url: SHOP };
  assert.deepEqual((await call(chrome, "scroll_page", { ...base, direction: "down", pages: 2 })).result, {
    status: "scrolled",
    origin: ORIGIN,
  });
  await call(chrome, "scroll_page", { ...base, direction: "up" });
  await call(chrome, "scroll_page", { ...base, direction: "top" });
  await call(chrome, "scroll_page", { ...base, direction: "bottom", pages: 5 });
  assert.deepEqual(fake.scrolls, [
    { call: "scrollBy", options: { top: 1600, behavior: "instant" } },
    { call: "scrollBy", options: { top: -800, behavior: "instant" } },
    { call: "scrollTo", options: { top: 0, behavior: "instant" } },
    { call: "scrollTo", options: { top: 9000, behavior: "instant" } },
  ]);
  assert.equal(asked.length, 0);
  assert.equal(html, null);
  fake.href = "https://elsewhere.example/";
  const left = await call(chrome, "scroll_page", { ...base, direction: "down" });
  assert.match(left.error.message, /left the URL/);
});

// inPage cannot import, so it holds its own copy of the lists the tools check
// a call against. A role the tool accepts and the page does not know would be
// found by no element, and a key the page has no code for would be sent without one.
test("the lists inside the injected function are the lists the tools check a call against", () => {
  const source = inPage.toString();
  const roles = source.match(/const ROLES = \[([^\]]*)\]/)[1].match(/"[^"]*"/g);
  assert.deepEqual(roles.map((word) => JSON.parse(word)), [...ROLES]);
  const codes = source.match(/const KEY_CODES = \{([^}]*)\}/)[1];
  assert.deepEqual(
    [...codes.matchAll(/(\w+):/g)].map((found) => found[1]).sort(),
    [...KEYS].sort(),
  );
});

test("the words and numbers a call gives are checked before any tab or page is reached", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  window();
  const ok = { url: SHOP, role: "button", name: "Pay now" };
  const bad = [
    ["click_element", { ...ok, role: "slider" }],
    ["click_element", { ...ok, role: "Button" }],
    ["click_element", { ...ok, role: undefined }],
    ["click_element", { ...ok, name: "" }],
    ["click_element", { ...ok, name: "   " }],
    ["click_element", { ...ok, name: "n".repeat(201) }],
    ["click_element", { ...ok, name: 5 }],
    ["click_element", { ...ok, button: "middle" }],
    ["click_element", { ...ok, count: 3 }],
    ["click_element", { ...ok, count: 0 }],
    ["click_element", { ...ok, count: "2" }],
    ["click_element", { ...ok, button: "right", count: 2 }],
    ["click_element", { role: "button", name: "Pay now" }],
    ["click_element", { ...ok, url: "" }],
    ["type_text", { ...ok, role: "textbox" }],
    ["type_text", { ...ok, role: "textbox", text: "" }],
    ["type_text", { ...ok, role: "textbox", text: "t".repeat(2001) }],
    ["type_text", { ...ok, role: "textbox", text: 5 }],
    ["select_option", { ...ok, role: "combobox" }],
    ["select_option", { ...ok, role: "combobox", option: "o".repeat(201) }],
    ["scroll_page", { url: SHOP, direction: "left" }],
    ["scroll_page", { url: SHOP }],
    ["scroll_page", { url: SHOP, direction: "down", pages: 0 }],
    ["scroll_page", { url: SHOP, direction: "down", pages: 6 }],
    ["scroll_page", { url: SHOP, direction: "down", pages: 1.5 }],
    ["scroll_page", { url: SHOP, direction: "down", pages: "2" }],
    ["wait_for_element", { ...ok, seconds: 0 }],
    ["wait_for_element", { ...ok, seconds: 31 }],
    ["wait_for_element", { ...ok, seconds: 2.5 }],
    ["wait_for_element", { ...ok, seconds: "5" }],
    ["wait_for_element", { ...ok, role: "link;" }],
  ];
  for (const [method, params] of bad) {
    const reply = await call(chrome, method, params);
    assert.equal(reply.error?.code, -32602, `${method} ${JSON.stringify(params)}`);
  }
  assert.ok(!reached(chrome, "tabs.query"));
  assert.ok(!reached(chrome, "scripting.executeScript"));

  // The largest values are taken.
  assert.equal((await call(chrome, "click_element", { ...ok, name: "Pay now", count: 2 })).result?.status, "double clicked");
  assert.equal((await call(chrome, "scroll_page", { url: SHOP, direction: "down", pages: 5 })).result?.status, "scrolled");
});

test("a wait ends when exactly one element shows, asks nothing and acts on nothing", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  const asked = window();
  const reply = await call(chrome, "wait_for_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.deepEqual(reply.result, { status: "found", origin: ORIGIN });
  assert.equal(asked.length, 0);
  assert.deepEqual(happenings(fake), []);
  assert.deepEqual(chrome.injected.map((made) => made.args[0].step), ["find"]);
});

test("a wait looks again after each interval until one element shows, and fails when the seconds pass first", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const late = button("late", "Pay now", { style: { display: "none" } });
    const fake = shop([late]);
    const chrome = browser({ fake });
    const run = chrome.scripting.executeScript;
    let looks = 0;
    chrome.scripting.executeScript = async (options) => {
      looks += 1;
      if (looks === 3) {
        late.style.display = "block";
      }
      return run(options);
    };
    const waiting = call(chrome, "wait_for_element", { url: SHOP, role: "button", name: "Pay now", seconds: 5 });
    await settled();
    assert.equal(looks, 1);
    mock.timers.tick(WAIT_POLL_MS - 1);
    await settled();
    assert.equal(looks, 1, "looked again before the interval");
    mock.timers.tick(1);
    await settled();
    assert.equal(looks, 2);
    mock.timers.tick(WAIT_POLL_MS);
    const reply = await waiting;
    assert.equal(looks, 3);
    assert.equal(reply.result?.status, "found");

    // Two that stay are not one, and the time passing is a failure.
    const twins = shop([button("a", "Pay now"), button("b", "Pay now")]);
    const stuck = call(browser({ fake: twins }), "wait_for_element", { url: SHOP, role: "button", name: "Pay now", seconds: 1 });
    for (let at = 0; at < 1000; at += WAIT_POLL_MS) {
      await settled();
      mock.timers.tick(WAIT_POLL_MS);
    }
    await settled();
    mock.timers.tick(1);
    const failed = await stuck;
    assert.equal(
      failed.error?.message,
      `no single element with that role and that name appeared within 1 seconds: ${FAILURES.several(2)}`,
    );
  } finally {
    mock.timers.reset();
  }
});

test("a wait for an element that is never acted on, or on a page that left, fails at once", async () => {
  const fake = shop([field("password", { "aria-label": "Password" })]);
  const chrome = browser({ fake });
  const refused = await call(chrome, "wait_for_element", { url: SHOP, role: "textbox", name: "Password" });
  assert.match(refused.error.message, /password field/);
  assert.equal(chrome.injected.length, 1);
  fake.href = "https://elsewhere.example/";
  const left = await call(chrome, "wait_for_element", { url: SHOP, role: "textbox", name: "Password" });
  assert.match(left.error.message, /left the URL/);
});

// The tab is found as every other tool finds it, and the blocked and allowed
// lists are read before any tab is looked for.
test("the tab is found by its exact URL, and a URL that is at no tab, two tabs, a blocked site or no allowed site reaches no page", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const ok = { url: SHOP, role: "button", name: "Pay now" };
  const none = browser({ fake, tabs: [{ id: 1, url: `${SHOP}?a=1` }] });
  assert.equal((await call(none, "click_element", ok)).error.message, `no open tab is at ${SHOP}`);
  const twice = browser({ fake, tabs: [{ id: 1, url: SHOP }, { id: 2, url: SHOP }] });
  window();
  assert.equal(
    (await call(twice, "click_element", ok)).error.message,
    `2 open tabs are at ${SHOP}, so the URL does not name one tab`,
  );
  for (const chrome of [none, twice]) {
    assert.ok(!reached(chrome, "scripting.executeScript"));
  }

  const blocked = browser({ fake, stored: { [BLOCKED_SITES_KEY]: [ORIGIN] } });
  const refused = await call(blocked, "click_element", ok);
  assert.match(refused.error.message, /blocked sites list/);
  assert.ok(!reached(blocked, "tabs.query"));

  const limited = browser({ fake, stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: ["https://other.example"] } });
  const not = await call(limited, "click_element", ok);
  assert.match(not.error.message, /not in the allowed sites list/);
  assert.ok(!reached(limited, "tabs.query"));
  assert.ok(!reached(limited, "scripting.executeScript"));

  const allowed = browser({ fake, stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [ORIGIN] } });
  assert.equal((await call(allowed, "click_element", ok)).result?.status, "clicked");
});

// Where the browser lets the extension into file: and ftp: pages, or the
// permission covers every address, a tab at one is named by its URL as a web page
// is. The tools that read or act on a page's elements are for web pages, as the
// tools that capture one are, and refuse any other address before a tab is looked
// for.
test("the tools that read or act on the elements of a page refuse an address that is not HTTP or HTTPS before a tab is looked for", async () => {
  const fake = shop([button("pay", "Pay now")]);
  for (const url of [
    "file:///home/person/report.html",
    "ftp://files.example/report.html",
    "chrome://settings/",
    "about:blank",
    "data:text/html,x",
    "javascript:alert(1)",
  ]) {
    const named = { url };
    const params = {
      click_element: { ...named, role: "button", name: "Pay now" },
      type_text: { ...named, role: "textbox", name: "Pay now", text: "x" },
      select_option: { ...named, role: "combobox", name: "Pay now", option: "x" },
      press_key: { ...named, key: "Enter" },
      scroll_page: { ...named, direction: "down" },
      wait_for_element: { ...named, role: "button", name: "Pay now" },
      list_page_elements: named,
    };
    for (const [method, given] of Object.entries(params)) {
      const chrome = browser({
        fake,
        listing: true,
        tabs: [{ id: 1, windowId: 1, title: "Local", url }],
        stored: { [ALLOWED_SITES_KEY]: [] },
      });
      const asked = window();
      const reply = await call(chrome, method, given);
      assert.equal(reply.error?.message, "url must be an HTTP or HTTPS URL", `${method} ${url}`);
      assert.equal(reply.error.code, -32602, `${method} ${url}`);
      for (const api of ["tabs.query", "scripting.executeScript", "tabs.update"]) {
        assert.ok(!reached(chrome, api), `${method} ${url} reached ${api}`);
      }
      assert.equal(asked.length, 0, `${method} ${url}`);
    }
  }
  // An address on the web is still found.
  const chrome = browser({ fake, listing: true });
  assert.ok("result" in (await call(chrome, "list_page_elements", { url: SHOP })));
});

test("a tool is refused before the page is reached unless its tool, its group and its browser permissions are on", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const params = {
    click_element: { url: SHOP, role: "button", name: "Pay now" },
    type_text: { url: SHOP, role: "textbox", name: "Pay now", text: "x" },
    select_option: { url: SHOP, role: "combobox", name: "Pay now", option: "x" },
    press_key: { url: SHOP, key: "Enter" },
    scroll_page: { url: SHOP, direction: "down" },
    wait_for_element: { url: SHOP, role: "button", name: "Pay now" },
  };
  assert.deepEqual(Object.keys(params), INPUT_TOOLS);
  const without = (name) => {
    const held = allOptional();
    return { ...held, permissions: held.permissions.filter((made) => made !== name) };
  };
  const refusals = {
    "its tool is off": [{ stored: { [SETTINGS_KEY]: {} } }, /is turned off/],
    "its group is off": [{ stored: { [GROUP_SWITCHES_KEY]: {} } }, /group, which is turned off/],
    "scripting is not granted": [{ granted: without("scripting") }, /needs the input group's browser permissions/],
    "tabs is not granted": [{ granted: without("tabs") }, /needs the input group's browser permissions/],
    "no host is granted": [{ granted: { permissions: allOptional().permissions, origins: [] } }, /needs the input group's browser permissions/],
  };
  for (const [method, given] of Object.entries(params)) {
    for (const [why, [setup, message]] of Object.entries(refusals)) {
      const chrome = browser({ fake, ...setup });
      const asked = window();
      const reply = await call(chrome, method, given);
      assert.match(reply.error?.message ?? "", message, `${method}: ${why}`);
      assert.ok(!reached(chrome, "tabs.query"), `${method}: ${why}`);
      assert.ok(!reached(chrome, "scripting.executeScript"), `${method}: ${why}`);
      assert.equal(asked.length, 0);
      assert.equal((await activityLog(chrome))[0].outcome, "refused", `${method}: ${why}`);
    }
    // The group's lifetime ended, as a restart of the browser ends it.
    const restarted = browser({ fake });
    await restarted.storage.session.set({ [RUNS_KEY]: {} });
    const ended = await call(restarted, method, given);
    assert.match(ended.error?.message ?? "", /has ended/, method);
  }
});

test("the group is on until the browser restarts and is in no preset", () => {
  assert.equal(defaultLifetime("input"), UNTIL_RESTART);
  for (const members of Object.values(PRESETS)) {
    assert.ok(!members.includes("input"));
  }
  assert.deepEqual(GROUPS.input.permissions, ["tabs", "scripting"]);
  assert.equal(GROUPS.input.hosts, true);
  for (const tool of INPUT_TOOLS) {
    assert.equal(typeof TOOLS[tool], "function", tool);
  }
});

test("the page is reached only by the three fixed functions, in the isolated world, in the top frame of the tab, with one object of known fields", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  window();
  await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now", selector: "#pay", x: 5, y: 5, tabId: 9, script: "alert(1)" });
  await call(chrome, "scroll_page", { url: SHOP, direction: "down", evaluate: "1" });
  const [find, clear, act, scroll] = chrome.injected;
  for (const made of chrome.injected) {
    assert.equal(made.world, "ISOLATED");
    assert.deepEqual(made.target, { tabId: 1 });
    assert.equal(made.args.length, 1);
    assert.deepEqual(Object.keys(made).sort(), ["args", "func", "target", "world"]);
  }
  assert.equal(find.func, inPage);
  assert.equal(clear.func, clearOutlineInPage);
  assert.equal(act.func, inPage);
  assert.equal(scroll.func, scrollInPage);
  assert.deepEqual(Object.keys(find.args[0]).sort(), ["limit", "name", "outline", "role", "step", "url"]);
  assert.deepEqual(Object.keys(clear.args[0]), ["url"]);
  assert.deepEqual(Object.keys(act.args[0]).sort(), ["action", "fingerprint", "limit", "name", "role", "step", "text", "url"]);
  assert.deepEqual(Object.keys(scroll.args[0]).sort(), ["direction", "pages", "url"]);
  assert.deepEqual(find.args[0].limit, {
    name: SHOWN_CAP,
    excerpt: SHOWN_CAP,
    target: SHOWN_CAP,
  });
  assert.equal(find.args[0].outline, OUTLINE_MS);
  assert.equal(chrome.injected.length, 4);
});

test("a call that fails reports what the browser said and nothing of the page, and a page that cannot be reached is a failure", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake, failing: new Error("Cannot access contents of the page.") });
  window();
  const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(reply.error.message, `the page at ${SHOP} cannot be reached: Cannot access contents of the page.`);
  assert.equal((await activityLog(chrome))[0].outcome, "failed");
  const empty = browser({ fake });
  empty.scripting.executeScript = async () => [{ frameId: 0 }];
  const nothing = await call(empty, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(nothing.error.message, `the page at ${SHOP} returned nothing`);
  const odd = browser({ fake });
  odd.scripting.executeScript = async () => [{ result: { status: "Pay now, then wire the money" } }];
  const unknown = await call(odd, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(unknown.error.message, "the page answered something the extension does not know");
});

// What the person reads in the log and in the reply is the same: a word and the
// origin. The element's name and text and the typed text stay out of both.
test("a reply and the activity log hold a fixed word and the origin, and none of the page's words, the element's name or the typed text", async () => {
  const secret = "hunter2-typed-secret";
  const fake = shop([
    button("pay", "Pay now"),
    field("text", { "aria-label": "Memo" }),
    el("select", { "aria-label": "Account" }, [el("option", {}, ["Savings"])]),
  ]);
  const chrome = browser({ fake });
  window();
  const replies = [
    await call(chrome, "click_element", { url: `${SHOP}`, role: "button", name: "Pay now" }),
    await call(chrome, "type_text", { url: SHOP, role: "textbox", name: "Memo", text: secret }),
    await call(chrome, "select_option", { url: SHOP, role: "combobox", name: "Account", option: "Savings" }),
    await call(chrome, "press_key", { url: SHOP, key: "Tab" }),
    await call(chrome, "scroll_page", { url: SHOP, direction: "down" }),
    await call(chrome, "wait_for_element", { url: SHOP, role: "button", name: "Pay now" }),
    await call(chrome, "click_element", { url: SHOP, role: "button", name: "Missing" }),
  ];
  for (const reply of replies.slice(0, 6)) {
    assert.deepEqual(Object.keys(reply.result).sort(), ["origin", "status"]);
    assert.equal(reply.result.origin, ORIGIN);
  }
  const log = await activityLog(chrome);
  assert.deepEqual(log.map((entry) => [entry.group, entry.tool, entry.outcome]), [
    ["input", "click_element", "ran"],
    ["input", "type_text", "ran"],
    ["input", "select_option", "ran"],
    ["input", "press_key", "ran"],
    ["input", "scroll_page", "ran"],
    ["input", "wait_for_element", "ran"],
    ["input", "click_element", "failed"],
  ]);
  for (const entry of log) {
    assert.deepEqual(entry.origins, [ORIGIN]);
    assert.deepEqual(Object.keys(entry).sort(), ["group", "origins", "outcome", "time", "tool"]);
  }
  const kept = JSON.stringify([replies, log]);
  for (const word of [secret, "Pay now", "Memo", "Account", "Savings", "Missing", "cart"]) {
    assert.ok(!kept.includes(word), `${word} is in a reply or the log`);
  }
});

// The planner's words are compared with the page's names, and typed text is put
// in a field as text. Nothing it wrote is run, read as a selector or parsed.
const HOSTILE = [
  `"><img src=x onerror=alert(1)>`,
  "*, [x]",
  "a:not(b)",
  `'); alert(1); ('`,
  "</script><script>alert(1)</script>",
  "${1 + 1}",
  "\\",
  "constructor",
  "__proto__",
  "#pay",
  "button[name=x]",
];

const BANNED = [
  [/\beval\b/, "eval"],
  [/\bFunction\b/, "Function"],
  [/\binnerHTML\b/, "innerHTML"],
  [/\bouterHTML\b/, "outerHTML"],
  [/insertAdjacentHTML/, "insertAdjacentHTML"],
  [/createContextualFragment|DOMParser|\.write\s*\(/, "an HTML parser"],
  [/querySelector(All)?\s*\(/, "a selector API"],
  [/\.matches\s*\(|\.closest\s*\(/, "matches or closest"],
  [/getElementsBy/, "getElementsBy"],
  [/\bsetTimeout\b|\bsetInterval\b/, "a timer"],
  [/\bimport\s*\(|\bfetch\b|XMLHttpRequest|WebSocket/, "code or network access"],
  [/location\s*\.\s*(assign|replace)|location\s*=|\.href\s*=/, "a navigation"],
];

// The names of what in `source` a page could use to run or parse a string.
function bannedIn(source) {
  const found = BANNED.filter(([pattern]) => pattern.test(source)).map(([, name]) => name);
  const chromes = source.match(/\bchrome\b[^\s;,)]*/g) ?? [];
  if (chromes.some((use) => !use.startsWith("chrome?.dom?.openOrClosedShadowRoot"))) {
    found.push("chrome");
  }
  return found;
}

test("the injected functions hold no way to run, parse or select with a string", () => {
  for (const func of [inPage, scrollInPage, clearOutlineInPage]) {
    assert.deepEqual(bannedIn(func.toString()), [], func.name);
  }
  // The scan finds each of these, so a pass above is not a scan that sees nothing.
  const bad = {
    eval: "(request) => eval(request.name)",
    Function: "(request) => new Function(request.name)()",
    innerHTML: "(request) => { document.body.innerHTML = request.text; }",
    outerHTML: "(request) => { document.body.outerHTML = request.text; }",
    insertAdjacentHTML: "(r) => document.body.insertAdjacentHTML('beforeend', r.text)",
    "a selector API": "(r) => document.querySelector(r.name)",
    "matches or closest": "(r) => document.body.matches(r.name)",
    getElementsBy: "(r) => document.getElementsByClassName(r.name)",
    "a timer": "(r) => setTimeout(r.text, 1)",
    "a navigation": "(r) => { location.href = r.text; }",
    chrome: "(r) => chrome.runtime.sendMessage(r.text)",
  };
  for (const [name, source] of Object.entries(bad)) {
    assert.ok(bannedIn(source).length > 0, `the scan misses ${name}`);
  }
});

test("a word the planner wrote reaches only the comparison with names and, for typed text, the field", async () => {
  for (const word of HOSTILE) {
    const fake = shop([
      el("button", { "aria-label": word, id: "named" }),
      field("text", { "aria-label": word, id: "typed" }),
      el("select", { "aria-label": word, id: "chosen" }, [el("option", {}, [word])]),
      el("p", { id: "pay" }, ["pay"]),
    ]);
    const chrome = browser({ fake });
    window();
    const clicked = await call(chrome, "click_element", { url: SHOP, role: "button", name: word });
    assert.equal(clicked.result?.status, "clicked", word);
    assert.equal(eventsOf(fake, "click")[0].target, fake.element("named"), word);
    const typed = await call(chrome, "type_text", { url: SHOP, role: "textbox", name: word, text: word });
    assert.equal(typed.result?.status, "typed", word);
    assert.equal(fake.element("typed").value, word);
    const chosen = await call(chrome, "select_option", { url: SHOP, role: "combobox", name: word, option: word });
    assert.equal(chosen.result?.status, "selected", word);
    const missing = await call(chrome, "click_element", { url: SHOP, role: "link", name: word });
    assert.match(missing.error.message, /no element has/, word);
    assert.ok(!missing.error.message.includes(word), word);

    assert.deepEqual(fake.risky, [], `${word} was run, parsed or used as a selector`);
    // Only ids the page wrote were looked up; the planner's words were not.
    assert.deepEqual(fake.lookups, [], word);
    // The typed text is in the field and in the input event, and nowhere else.
    const holding = fake.events.filter((event) =>
      JSON.stringify([event.type, event.value ?? "", event.event?.data ?? ""]).includes(
        JSON.stringify(word).slice(1, -1),
      ),
    );
    assert.deepEqual(
      holding.map((event) => event.type).sort(),
      ["input", "value"],
      word,
    );
  }
});

// What a page's markup can do to a node: a form answers a property of its own
// with the control it holds that has that name, so a control named "action"
// stands in for form.action.
const PROPERTIES = [
  "action",
  "method",
  "elements",
  "length",
  "parentElement",
  "parentNode",
  "children",
  "childNodes",
  "id",
  "name",
  "tagName",
  "localName",
  "nodeType",
  "attributes",
  "getAttribute",
  "hasAttribute",
  "getRootNode",
  "contains",
  "hidden",
  "textContent",
  "value",
];

// A control that holds no value and takes no room, named for a property.
const controlNamed = (name) =>
  el("input", { type: "hidden", name }, [], { box: { left: 0, top: 0, width: 0, height: 0 } });

test("a form with a control named like a property of the form still shows the address it goes to, and a list leaves it out when that address is blocked", async () => {
  const make = () => {
    const submit = button("cancel", "Cancel");
    const form = el(
      "form",
      { id: "pay", action: "/evil-pay", method: "post" },
      [...PROPERTIES.map(controlNamed), submit],
      { action: "https://evil.example/evil-pay" },
    );
    submit.form = form;
    const fake = shop([form]);
    fake.clobberByName();
    return fake;
  };
  const named = make();
  assert.notEqual(named.element("pay").action, "https://evil.example/evil-pay", "the fake form is not clobbered");
  const asked = window(() => null);
  const clicked = await call(browser({ fake: named, listing: true }), "click_element", {
    url: SHOP,
    role: "button",
    name: "Cancel",
  });
  assert.equal(clicked.error?.message, "the action was not allowed in the browser, so nothing was done");
  assert.equal(asked[0].description.goes, "https://evil.example/evil-pay");
  assert.equal(asked[0].description.tag, "button");
  const listed = rowsOf(await listWith(browser({ fake: make(), listing: true })));
  assert.deepEqual(listed, [["1", "button", "Cancel", "https://evil.example"]]);
  const blocked = rowsOf(
    await listWith(
      browser({
        fake: make(),
        listing: true,
        stored: { [BLOCKED_SITES_KEY]: ["https://evil.example"] },
      }),
    ),
  );
  assert.deepEqual(blocked, [], "a control that submits to a blocked site was listed");
  window();
  const done = make();
  const allowed = await call(browser({ fake: done }), "click_element", {
    url: SHOP,
    role: "button",
    name: "Cancel",
  });
  assert.equal(allowed.result?.status, "clicked", allowed.error?.message);
  assert.equal(eventsOf(done, "click")[0].target, done.element("cancel"));
});

const rowsOf = (text) =>
  text
    .split("\n")
    .slice(2)
    .map((line) => line.split("\t"));

const listWith = async (chrome) => {
  const listed = await call(chrome, "list_page_elements", { url: SHOP });
  assert.equal(typeof listed.result, "string", listed.error?.message);
  return listed.result;
};

// A page with a node of every kind the injected functions read: names from a
// label, from aria-labelledby and from content, fields of every kind, a select
// with options, an editing host, a form with a button that has a form action of
// its own, a disabled fieldset, hidden elements, and an open and a closed shadow
// root. Two are made so that one can be clobbered and the other not.
function crowded() {
  const form = el("form", { id: "pay", action: "/charge", method: "post" }, [], {
    action: "https://shop.example/charge",
  });
  const buy = button("buy", "Buy");
  const cancel = button("cancel", "Cancel");
  cancel.attributes.formaction = "/refund";
  cancel.formAction = "https://shop.example/refund";
  buy.form = form;
  cancel.form = form;
  form.append(...PROPERTIES.map(controlNamed), buy, cancel);
  const email = el("input", { id: "email", type: "email" });
  const billing = el("input", { id: "billing", type: "text", "aria-labelledby": "t" });
  const choice = el("select", { id: "size", "aria-label": "Size" }, [
    el("option", { value: "s" }, ["Small"]),
    el("option", { value: "l" }, ["Large"]),
    el("option", { value: "x" }, ["Closed"], { disabled: true }),
  ]);
  const note = el("div", { id: "note", contenteditable: "true", "aria-label": "Note" });
  const shadowButton = button("inner", "Inside");
  const closedButton = button("sealed", "Sealed");
  return shop([
    el("h1", {}, ["Cart"]),
    el("label", { for: "email" }, ["Email address"]),
    email,
    el("span", { id: "t" }, ["Billing ", el("b", {}, ["address"])]),
    billing,
    el("a", { id: "go", href: "/ok" }, [el("img", { alt: "Home" })], { href: "https://shop.example/ok" }),
    el("a", { id: "out", href: "/x" }, ["Elsewhere"], { href: "https://elsewhere.example/x" }),
    form,
    choice,
    note,
    el("textarea", { id: "bio", "aria-label": "Bio" }, [], { held: "" }),
    el("fieldset", {}, [el("input", { type: "text", "aria-label": "Locked" })], { disabled: true }),
    el("div", { hidden: "" }, [button("ghost", "Ghost")], { hidden: true }),
    el("input", { type: "password", "aria-label": "Secret" }),
    shadowed(el("div", { id: "host" }), "open", [shadowButton]),
    shadowed(el("div", { id: "seal" }), "closed", [closedButton]),
  ]);
}

// Everything the tools do to a page, as what each call answered, what the person
// was shown and what the page saw.
async function everythingDone(fake) {
  const chrome = browser({ fake, listing: true });
  const asked = window();
  const answers = [];
  const tell = async (method, params) => {
    const reply = await call(chrome, method, { url: SHOP, ...params });
    answers.push([method, reply.result ?? reply.error.message]);
  };
  await tell("list_page_elements", {});
  await tell("click_element", { role: "button", name: "Buy" });
  await tell("click_element", { role: "button", name: "Cancel" });
  await tell("click_element", { role: "link", name: "Home" });
  await tell("click_element", { role: "button", name: "Inside" });
  await tell("click_element", { role: "button", name: "Sealed" });
  await tell("click_element", { role: "button", name: "Ghost" });
  await tell("click_element", { role: "button", name: "Buy", count: 2 });
  await tell("click_element", { role: "button", name: "Buy", button: "right" });
  await tell("click_element", { element_number: 2 });
  await tell("type_text", { role: "textbox", name: "Email address", text: "a@b.example" });
  await tell("type_text", { role: "textbox", name: "Billing address", text: "1 Main St" });
  await tell("type_text", { role: "textbox", name: "Note", text: "hello" });
  await tell("type_text", { role: "textbox", name: "Locked", text: "no" });
  await tell("type_text", { role: "textbox", name: "Secret", text: "no" });
  await tell("select_option", { role: "combobox", name: "Size", option: "Large" });
  await tell("select_option", { role: "combobox", name: "Size", option: "Closed" });
  fake.active = fake.element("email");
  await tell("press_key", { key: "Enter" });
  fake.active = null;
  await tell("press_key", { key: "Tab" });
  await tell("scroll_page", { direction: "bottom" });
  await tell("scroll_page", { direction: "down", pages: 2 });
  await tell("wait_for_element", { role: "button", name: "Buy", seconds: 1 });
  return {
    answers,
    shown: asked.map((resolution) => resolution.description),
    events: fake.events.map((event) => [
      event.type,
      attributesOf(event.target).id ?? attributesOf(event.target).name ?? null,
      event.value ?? event.event?.key ?? null,
    ]),
    scrolls: fake.scrolls,
    outlines: fake.animations.length,
    outlinesLeft: fake.added().length,
    removed: fake.removed.length,
  };
}

test("a page that puts something in front of every property of every node is read, listed and acted on exactly as one that does not", async () => {
  const clean = await everythingDone(crowded());
  assert.ok(clean.shown.length >= 10, "the page was not acted on");
  assert.ok(clean.events.length >= 30);
  for (const mode of ["clobberEverything", "clobberByName"]) {
    const fake = crowded();
    fake[mode](["formAction", "scrollHeight", "innerText", "title", "selected", "options"]);
    assert.notEqual(fake.element("pay").action, "https://shop.example/charge", mode);
    assert.deepEqual(await everythingDone(fake), clean, mode);
  }
});

test("each function injected into a page reads the page's nodes only through the prototype, with the three lines pageReads writes", () => {
  const strip = (source) =>
    source
      .replace(/\/\/[^\n]*/g, "")
      .replace(/"(?:[^"\\\n]|\\.)*"/g, '""')
      .replace(/'(?:[^'\\\n]|\\.)*'/g, "''")
      .replace(/\/(?![*/])(?:[^/\\\n[]|\\.|\[(?:[^\]\\]|\\.)*\])+\/[a-z]*/g, "/re/")
      .replace(/`(?:[^`\\]|\\.)*`/g, (text) => `\`${(text.match(/\$\{[^}]*\}/g) ?? []).join("")}\``);
  // A property read of a node is the name of a node on the left of a dot, or the
  // name of a property that only nodes and documents have on the right of one.
  // `request.action` is the one name besides these that a node shares.
  const BASES =
    /\b(?:element|node|child|parent|root|active|top|tree|host|frame|option|made|form|shadow|document|inner|next|up)\s*\??\./;
  const PROPERTIES_OF_NODES = new RegExp(
    `(?<!request)\\.\\s*(?:action|method|elements|parentElement|parentNode|children|childNodes|tagName|localName|nodeType|attributes|getAttribute|hasAttribute|getRootNode|contains|textContent|innerText|form|formAction|labels|options|shadowRoot|activeElement|documentElement|body|title|disabled|readOnly|multiple|dispatchEvent|focus|scrollIntoView|getBoundingClientRect|scrollHeight|getElementById|elementFromPoint|execCommand|createElement|attachShadow|append|animate|selected|data|id|host|value)\\b`,
  );
  const nodeReads = (source) => {
    const code = strip(source);
    return [code.match(BASES)?.[0], code.match(PROPERTIES_OF_NODES)?.[0]].filter(Boolean);
  };
  const canonical = pageReads
    .toString()
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => /^const (read|call|put) =/.test(line));
  assert.equal(canonical.length, 3);
  for (const func of [inPage, scrollInPage, clearOutlineInPage, listInPage, readInPage]) {
    const source = func.toString();
    assert.deepEqual(nodeReads(source), [], `${func.name} reads a node by name`);
    const lines = source.split("\n").map((line) => line.trim());
    for (const line of canonical) {
      const used = new RegExp(`\\b${line.match(/^const (\w+)/)[1]}\\(`).test(strip(source).replace(line, ""));
      assert.equal(lines.includes(line), used || lines.includes(line), func.name);
      if (used) {
        assert.ok(lines.includes(line), `${func.name} uses ${line.slice(6, 10)} without pageReads' line`);
      }
    }
  }
  // The scan finds each of these, so a pass above is not a scan that sees nothing.
  const bad = [
    "(form) => form.action",
    "(node) => node.parentElement",
    "(node) => node.getAttribute('x')",
    "() => document.body",
    "(node) => node.localName === 'a'",
    "(node) => node.textContent",
    "(select) => select.options",
    "(root) => root.children",
  ];
  for (const source of bad) {
    assert.ok(nodeReads(source).length > 0, `the scan misses ${source}`);
  }
});

test("a page whose elements are each other's parents does not hold a list or an action for ever", async () => {
  // A browser has no such tree. The loops up the tree end by the number of
  // elements, so a parent that is never null cannot hold the tab.
  const fake = shop([el("div", { id: "outer" }, [button("pay", "Pay now")])]);
  const chrome = browser({ fake, listing: true });
  const [outer, pay] = [fake.element("outer"), fake.element("pay")];
  const parents = new Map([[outer, pay], [pay, outer]]);
  const own = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(outer), "parentElement");
  Object.defineProperty(Object.getPrototypeOf(outer), "parentElement", {
    get() {
      return parents.get(this) ?? own.get.call(this);
    },
    configurable: true,
  });
  try {
    window();
    const listed = await call(chrome, "list_page_elements", { url: SHOP });
    assert.equal(typeof listed.result, "string", listed.error?.message);
    const clicked = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.equal(clicked.result?.status, "clicked", clicked.error?.message);
  } finally {
    Object.defineProperty(Object.getPrototypeOf(outer), "parentElement", own);
  }
});

test("the outline of an earlier action is looked for among the world's own globals, so an element of the page with the id of the global is not taken for it", async () => {
  // A browser answers a global that does not exist with the element of the page
  // that has that id.
  Object.defineProperty(Object.prototype, "bravebotOutline", {
    get: () => CONTROL,
    set(value) {
      Object.defineProperty(this, "bravebotOutline", { value, writable: true, configurable: true });
    },
    configurable: true,
  });
  try {
    const fake = shop([button("pay", "Pay now")]);
    const chrome = browser({ fake });
    window();
    const clicked = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.equal(clicked.result?.status, "clicked", clicked.error?.message);
  } finally {
    delete Object.prototype.bravebotOutline;
  }
});

test("a page that never lets an injected function finish fails the call after 15 seconds, and the action after it is not refused as busy", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const calls = {
      click_element: { role: "button", name: "Pay now" },
      type_text: { role: "textbox", name: "Pay now", text: "x" },
      scroll_page: { direction: "down" },
      list_page_elements: {},
      read_page: {},
      read_page_links: {},
      read_page_forms: {},
    };
    const fake = shop([button("pay", "Pay now")]);
    const chrome = browser({ fake, listing: true, hanging: true });
    window();
    for (const [method, params] of Object.entries(calls)) {
      const pending = call(chrome, method, { url: SHOP, ...params });
      await new Promise((resolve) => setImmediate(resolve));
      await new Promise((resolve) => setImmediate(resolve));
      mock.timers.tick(PAGE_WAIT_MS - 1);
      await new Promise((resolve) => setImmediate(resolve));
      mock.timers.tick(1);
      const reply = await pending;
      assert.match(reply.error?.message ?? "", /did not answer within 15 seconds/, method);
      assert.deepEqual(happenings(fake), [], method);
    }
    chrome.state.hanging = false;
    const clicked = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
    assert.equal(clicked.result?.status, "clicked", clicked.error?.message);
  } finally {
    mock.timers.reset();
  }
});

// Where acting on an element can lead is worked out in the page from the element
// and every element above it, as the click that is sent to the element goes up the
// page: a link around it, a button that is it or is around it, and for a field the
// form's first submit button. The click is sent to the element, so a link inside it
// is not followed and is not shown.
test("the window shows where a click goes whichever element above the one shown decides it: a link around it, a button around it, the first submit button of the form for a field, and unknown for an image map", async () => {
  const leave = (more = {}) => el("a", { href: "/out", ...more }, [], { href: "https://shop.example/out" });
  const goesOf = async (fake, role, name, method = "click_element", more = {}) => {
    const asked = window(() => null);
    const reply = await call(browser({ fake }), method, { url: SHOP, role, name, ...more });
    assert.ok(asked.length > 0, `${name}: ${reply.error?.message}`);
    return asked[0].description;
  };
  // A link around the element.
  const around = leave();
  around.append(el("div", { role: "button", "aria-label": "Cancel", tabindex: "0" }, ["Cancel"]));
  assert.equal((await goesOf(shop([around]), "button", "Cancel")).goes, "https://shop.example/out");
  // A link inside the element: the click is sent to the element and goes no further down.
  const inside = el("div", { role: "button", "aria-label": "Cancel", tabindex: "0" }, ["Cancel"]);
  inside.append(leave());
  const within = await goesOf(shop([inside]), "button", "Cancel");
  assert.equal(within.goes, "");
  assert.equal(within.goesUnknown, undefined);
  // The innermost decides: a button around the element, in a form, inside a link.
  const form = el("form", { id: "pay" }, [], { action: "https://shop.example/charge" });
  const wrapper = el("button", { id: "wrap", form: "pay", "aria-label": "Wrapper" }, [], { form });
  wrapper.append(el("span", { role: "button", "aria-label": "Inner" }, ["Inner"]));
  const outer = leave();
  outer.append(wrapper);
  assert.equal((await goesOf(shop([form, outer]), "button", "Inner")).goes, "https://shop.example/charge");
  // A button of a type that does not submit passes the click on to the link around it.
  const plain = button("plain", "Plain", { form });
  plain.attributes.type = "button";
  const link = leave();
  link.append(plain);
  assert.equal((await goesOf(shop([form, link]), "button", "Plain")).goes, "https://shop.example/out");
  // A field: Enter goes where the form's first submit button sends it, with its formaction in place
  // of the form's action, and to the form's action where it has no submit button.
  const searching = el("form", { id: "search" }, [], { action: "https://shop.example/search" });
  const query = el("input", { type: "text", "aria-label": "Search" }, [], { form: searching });
  const first = button("go", "Go", { form: searching });
  first.attributes.formaction = "/pay";
  Object.assign(first, { formAction: "https://shop.example/pay" });
  const second = button("also", "Also", { form: searching });
  assert.equal((await goesOf(shop([searching, query, first, second]), "textbox", "Search")).goes, "https://shop.example/pay");
  const unformaction = button("go", "Go", { form: searching });
  assert.equal((await goesOf(shop([searching, query, unformaction]), "textbox", "Search")).goes, "https://shop.example/search");
  second.attributes.type = "button";
  const noButton = el("input", { type: "text", "aria-label": "Search" }, [], { form: searching });
  const lone = shop([searching, noButton, el("button", { id: "x", type: "button" }, ["X"], { form: searching })]);
  assert.equal((await goesOf(lone, "textbox", "Search")).goes, "https://shop.example/search");
  // A field inside a link, in a form that goes elsewhere: two places.
  const linked = leave();
  linked.append(el("input", { type: "text", "aria-label": "Search" }, [], { form: searching }));
  const two = await goesOf(shop([searching, linked]), "textbox", "Search");
  assert.equal(two.goes, "");
  assert.equal(two.goesUnknown, true);
  // An anchor with no address is not a link, and an input of type hidden is not submitted by Enter.
  const bare = () => {
    const anchor = el("a", {}, [], { href: "" });
    anchor.append(button("anchored", "Anchored"));
    return anchor;
  };
  assert.equal((await goesOf(shop([bare()]), "button", "Anchored")).goes, "");
  const beyond = leave();
  beyond.append(bare());
  assert.equal((await goesOf(shop([beyond]), "button", "Anchored")).goes, "https://shop.example/out", "the link beyond it decides");
  const hidden = el("input", { type: "hidden", role: "textbox", "aria-label": "Token" }, [], { form: searching });
  assert.equal((await goesOf(shop([searching, hidden]), "textbox", "Token")).goes, "");
  // An image map: its links are not placed in the page.
  const mapped = await goesOf(shop([el("img", { role: "button", "aria-label": "Zones", usemap: "#zones" })]), "button", "Zones");
  assert.equal(mapped.goes, "");
  assert.equal(mapped.goesUnknown, true);
  for (const value of Object.values(mapped)) {
    assert.ok(["string", "object", "boolean"].includes(typeof value));
  }
});

test("a list and the blocked sites judge an element by the same place it goes: an element in a link to a blocked site is left out, so is one whose place cannot be told while a site is blocked, and a field is judged by its form's first submit button", async () => {
  const make = () => {
    const bank = el("a", { href: "/bank" }, [], { href: "https://bank.example/login" });
    bank.append(el("div", { role: "button", "aria-label": "Cancel" }, ["Cancel"]));
    const form = el("form", { id: "search" }, [], { action: "https://shop.example/search" });
    const field = el("input", { type: "text", "aria-label": "Find" }, [], { form });
    const submit = button("go", "Go", { form });
    submit.attributes.formaction = "https://bank.example/pay";
    Object.assign(submit, { formAction: "https://bank.example/pay" });
    const mapped = el("img", { role: "button", "aria-label": "Zones", usemap: "#zones" });
    return shop([bank, form, field, submit, mapped]);
  };
  const open = rowsOf(await listWith(browser({ fake: make(), listing: true })));
  assert.deepEqual(open, [
    ["1", "link", "Cancel", "https://bank.example"],
    ["2", "button", "Cancel", "https://bank.example"],
    ["3", "textbox", "Find", "https://bank.example"],
    ["4", "button", "Go", "https://bank.example"],
    ["5", "button", "Zones", "unknown"],
  ]);
  const blocked = rowsOf(
    await listWith(
      browser({ fake: make(), listing: true, stored: { [BLOCKED_SITES_KEY]: ["https://bank.example"] } }),
    ),
  );
  assert.deepEqual(blocked, [], "an element that goes to the blocked site, or may, was listed");
  const elsewhere = rowsOf(
    await listWith(
      browser({ fake: make(), listing: true, stored: { [BLOCKED_SITES_KEY]: ["https://other.example"] } }),
    ),
  );
  assert.deepEqual(
    elsewhere.map((row) => row[2]),
    ["Cancel", "Cancel", "Find", "Go"],
    "a site that is blocked and not the one it goes to leaves out only what cannot be told",
  );
  window();
});

test("where a click goes counts in what the person was shown, so a link put around the element after the window is not clicked", async () => {
  const fake = shop([button("pay", "Pay now")]);
  const chrome = browser({ fake });
  const asked = window((resolution) => {
    const link = el("a", { href: "/out" }, [], { href: "https://shop.example/out" });
    const target = fake.element("pay");
    link.append(target);
    fake.element("pay").parentNode = link;
    return approve(resolution);
  });
  const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Pay now" });
  assert.equal(asked[0].description.goes, "");
  assert.equal(reply.error?.message, FAILURES.changed(0));
});

test("that where a click goes cannot be told is part of what was shown, so an image map that stops being one after the window is not clicked", async () => {
  const fake = shop([el("img", { id: "zones", role: "button", "aria-label": "Zones", usemap: "#zones" })]);
  const chrome = browser({ fake });
  const asked = window((resolution) => {
    delete fake.element("zones").attributes.usemap;
    return approve(resolution);
  });
  const reply = await call(chrome, "click_element", { url: SHOP, role: "button", name: "Zones" });
  assert.equal(asked[0].description.goesUnknown, true);
  assert.equal(reply.error?.message, FAILURES.changed(0));
});

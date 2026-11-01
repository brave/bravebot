// The five methods, the arguments each takes, and the shape of a reply, against
// handlers that record what they are given.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as input from "../../extension/input.js";
import { PATH_LIMIT, problemWithFile } from "../files.js";
import { HANDLERS, METHODS, STATUSES, VERSION, answer, parse } from "../requests.js";
import { ARGUMENTS, Failure, Refusal, SENTENCES } from "../sentences.js";
import { TOOLS } from "../tools.js";
import * as words from "../words.js";
import { fakeDebugger } from "./fake-debugger.mjs";

const URL_A = "https://example.test/page";
const ORIGIN_A = "https://example.test";

// Handlers that record each call and answer that the method was done.
function recording(status = "clicked") {
  const calls = [];
  const handlers = {};
  for (const method of Object.keys(METHODS)) {
    handlers[method] = async (args, chrome) => {
      calls.push({ method, args, chrome });
      return { status, origin: ORIGIN_A };
    };
  }
  return { calls, handlers };
}

// A request of `method` with `params`.
const request = (method, params) => ({ v: VERSION, method, params });

// The good arguments of each method.
const GOOD = {
  click_element_trusted: { url: URL_A, role: "button", name: "Save" },
  type_text_trusted: { url: URL_A, role: "textbox", name: "Name", text: "Ada" },
  press_key_trusted: { url: URL_A, key: "Enter" },
  hover_element: { url: URL_A, role: "link", name: "Docs" },
  drag_element: {
    url: URL_A,
    role: "option",
    name: "One",
    onto_role: "option",
    onto_name: "Two",
  },
};

// The reply to `message` with handlers that record, and the calls.
async function ask(message) {
  const { calls, handlers } = recording();
  const reply = await answer(message, "the browser", handlers);
  return { reply, calls };
}

test("the table holds the eight methods and no other, and is frozen", () => {
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
  assert.deepEqual(Object.keys(HANDLERS), Object.keys(METHODS));
  assert.ok(Object.isFrozen(METHODS) && Object.isFrozen(HANDLERS));
  for (const method of Object.values(METHODS)) {
    assert.ok(Object.isFrozen(method) && Object.isFrozen(method.required));
  }
});

test("a method gets the arguments it takes, with its defaults, and no others", async () => {
  for (const [method, params] of Object.entries(GOOD)) {
    const { reply, calls } = await ask(request(method, params));
    assert.equal(calls.length, 1, method);
    assert.equal(calls[0].method, method);
    assert.equal(calls[0].chrome, "the browser");
    const defaults = method === "click_element_trusted" ? { button: "left", count: 1 } : {};
    assert.deepEqual({ ...calls[0].args }, { ...params, ...defaults });
    assert.ok(Object.isFrozen(calls[0].args));
    assert.deepEqual(reply, { v: 1, result: { status: "clicked", origin: ORIGIN_A } });
  }
  const { calls } = await ask(
    request("click_element_trusted", { ...GOOD.click_element_trusted, button: "left", count: 2 }),
  );
  assert.deepEqual({ ...calls[0].args }, { ...GOOD.click_element_trusted, button: "left", count: 2 });
  const right = await ask(
    request("click_element_trusted", { ...GOOD.click_element_trusted, button: "right" }),
  );
  assert.equal(right.calls[0].args.button, "right");
  assert.equal(right.calls[0].args.count, 1);
});

test("a method outside the table is refused and no handler runs", async () => {
  for (const method of [
    "click_element",
    "type_text",
    "press_key",
    "select_option",
    "scroll_page",
    "Click_Element_Trusted",
    "click_element_trusted ",
    "toString",
    "constructor",
    "__proto__",
    "hasOwnProperty",
    "",
    undefined,
    null,
    7,
    ["hover_element"],
    { toString: () => "hover_element" },
  ]) {
    const { reply, calls } = await ask({ v: VERSION, method, params: GOOD.hover_element });
    assert.deepEqual(calls, [], `${String(method)} ran a handler`);
    assert.deepEqual(reply, {
      v: 1,
      error: { kind: "refusal", message: SENTENCES.request() },
    });
  }
});

test("a message that is not a version 1 request of a method and its arguments is refused", async () => {
  const good = request("hover_element", GOOD.hover_element);
  for (const message of [
    undefined,
    null,
    "hover_element",
    7,
    [],
    [good],
    {},
    { ...good, v: 2 },
    { ...good, v: "1" },
    { method: good.method, params: good.params },
    { ...good, id: 4 },
    { ...good, tabId: 4 },
    { ...good, params: [] },
    { ...good, params: "url" },
  ]) {
    const { reply, calls } = await ask(message);
    assert.deepEqual(calls, [], JSON.stringify(message));
    assert.deepEqual(reply, {
      v: 1,
      error: { kind: "refusal", message: SENTENCES.request() },
    });
  }
});

// Values an argument has to refuse, for every method that takes it.
const BAD = {
  url: [
    undefined,
    null,
    7,
    "",
    "  ",
    "example.test",
    "chrome://version",
    "file:///etc/passwd",
    "about:blank",
    "javascript:alert(1)",
    ["https://example.test/"],
  ],
  role: [
    undefined,
    "",
    "heading",
    "Button",
    "button ",
    "button,link",
    "[role=button]",
    "toString",
    ["button"],
    7,
  ],
  name: [undefined, "", "   ", "x".repeat(201), 7, null, ["Save"]],
  onto_role: [undefined, "", "heading", "Option", 7, ["option"]],
  onto_name: [undefined, "", "   ", "y".repeat(201), 7, null],
  text: [undefined, "", "   ", "t".repeat(2001), 7, null, ["a"]],
  key: [
    undefined,
    "",
    "a",
    "enter",
    "Enter ",
    "Control+a",
    "Shift",
    "F5",
    "Enter,Tab",
    ["Enter"],
    7,
  ],
  button: ["middle", "Left", "", 1, null, ["left"]],
  count: [0, 3, -1, 1.5, "1", "2", null, NaN, Infinity, [1]],
};

test("an argument that is not a word of its list or is out of bounds is refused, and the reply is the sentence for that argument and nothing it held", async () => {
  for (const [method, params] of Object.entries(GOOD)) {
    const names = [
      ...Object.keys(params),
      ...(method === "click_element_trusted" ? ["button", "count"] : []),
    ];
    for (const name of names) {
      for (const bad of BAD[name]) {
        const given = { ...params };
        if (bad === undefined) {
          delete given[name];
        } else {
          given[name] = bad;
        }
        // A button or a count that is left out has a default, and is not bad.
        if (bad === undefined && (name === "button" || name === "count")) {
          continue;
        }
        const { reply, calls } = await ask(request(method, given));
        assert.deepEqual(calls, [], `${method} ${name}=${JSON.stringify(bad)} ran`);
        assert.deepEqual(reply, {
          v: 1,
          error: { kind: "refusal", message: SENTENCES.argument(name) },
        });
      }
    }
  }
});

test("the bounds are the edges: 200 characters of a name, 2000 of a text, a count of 1 or 2", async () => {
  const name = "n".repeat(200);
  const text = "t".repeat(2000);
  for (const [method, params] of [
    ["hover_element", { ...GOOD.hover_element, name }],
    ["type_text_trusted", { ...GOOD.type_text_trusted, text }],
    ["drag_element", { ...GOOD.drag_element, name, onto_name: name }],
    ["click_element_trusted", { ...GOOD.click_element_trusted, count: 1 }],
    ["click_element_trusted", { ...GOOD.click_element_trusted, count: 2 }],
  ]) {
    const { calls } = await ask(request(method, params));
    assert.equal(calls.length, 1, `${method} refused an argument at its bound`);
  }
  for (const key of words.KEYS) {
    assert.equal((await ask(request("press_key_trusted", { url: URL_A, key }))).calls.length, 1, key);
  }
  for (const role of words.ROLES) {
    assert.equal(
      (await ask(request("hover_element", { ...GOOD.hover_element, role }))).calls.length,
      1,
      role,
    );
  }
});

test("a right click is made once", async () => {
  const { reply, calls } = await ask(
    request("click_element_trusted", { ...GOOD.click_element_trusted, button: "right", count: 2 }),
  );
  assert.deepEqual(calls, []);
  assert.deepEqual(reply.error, { kind: "refusal", message: SENTENCES.argument("count") });
});

test("an argument the method does not take is refused whatever it is called, and is not named", async () => {
  const extras = [
    "selector",
    "x",
    "y",
    "tabId",
    "tab_id",
    "nodeId",
    "backendNodeId",
    "script",
    "code",
    "expression",
    "frameId",
    "offset",
    "__proto__",
    "constructor",
  ];
  for (const [method, params] of Object.entries(GOOD)) {
    for (const extra of extras) {
      const given = JSON.parse(JSON.stringify(params));
      Object.defineProperty(given, extra, { value: "1", enumerable: true, configurable: true });
      const { reply, calls } = await ask(request(method, given));
      assert.deepEqual(calls, [], `${method} took ${extra}`);
      assert.deepEqual(reply.error, { kind: "refusal", message: SENTENCES.extra() });
      assert.equal(reply.error.message.includes(extra), false);
    }
  }
  // An argument another method takes.
  for (const [method, other] of [
    ["hover_element", "text"],
    ["hover_element", "key"],
    ["click_element_trusted", "onto_role"],
    ["press_key_trusted", "role"],
    ["type_text_trusted", "button"],
    ["drag_element", "count"],
  ]) {
    const { calls } = await ask(request(method, { ...GOOD[method], [other]: GOOD.drag_element[other] ?? "left" }));
    assert.deepEqual(calls, [], `${method} took ${other}`);
  }
});

test("the lists and limits are those the BraveBot extension checks its own arguments against", () => {
  for (const name of [
    "ROLES",
    "KEYS",
    "DIRECTIONS",
    "BUTTONS",
    "NAME_LIMIT",
    "TYPED_TEXT_LIMIT",
    "EXCERPT_LIMIT",
    "TARGET_LIMIT",
    "URL_LIMIT",
    "SHOWN_CAP",
  ]) {
    assert.deepEqual(words[name], input[name], name);
  }
  assert.deepEqual(Object.keys(words).sort(), [
    "BUTTONS",
    "DIRECTIONS",
    "EXCERPT_LIMIT",
    "KEYS",
    "NAME_LIMIT",
    "ROLES",
    "SHOWN_CAP",
    "TARGET_LIMIT",
    "TYPED_TEXT_LIMIT",
    "URL_LIMIT",
  ]);
});

test("a reply holds a status word and an origin, and nothing else the method returned", async () => {
  for (const status of STATUSES) {
    const handlers = {
      hover_element: async () => ({
        status,
        origin: ORIGIN_A,
        title: "SECRET page title",
        text: "SECRET page text",
        tabId: 7,
      }),
    };
    const reply = await answer(request("hover_element", GOOD.hover_element), null, handlers);
    assert.deepEqual(reply, { v: 1, result: { status, origin: ORIGIN_A } });
  }
});

test("what a method returns that is not a status word and an origin is a failure with the one sentence", async () => {
  for (const returned of [
    undefined,
    null,
    "clicked",
    {},
    { status: "clicked" },
    { origin: ORIGIN_A },
    { status: "<img src=x onerror=alert(1)>", origin: ORIGIN_A },
    { status: "Clicked", origin: ORIGIN_A },
    { status: "clicked", origin: `${ORIGIN_A}/path` },
    { status: "clicked", origin: "https://example.test/?a=SECRET" },
    { status: "clicked", origin: "javascript:alert(1)" },
    { status: "clicked", origin: "null" },
    { status: "clicked", origin: "SECRET" },
    { status: "clicked", origin: 7 },
  ]) {
    const handlers = { hover_element: async () => returned };
    const reply = await answer(request("hover_element", GOOD.hover_element), null, handlers);
    assert.deepEqual(reply, { v: 1, error: { kind: "failed", message: SENTENCES.failed() } });
  }
});

test("an error a method throws is sent as its code's sentence or the generic one, never its own words", async () => {
  const secret = "SECRET page title and <b>markup</b>";
  const thrown = [
    [new Error(secret), "failed", SENTENCES.failed()],
    [new TypeError(secret), "failed", SENTENCES.failed()],
    [secret, "failed", SENTENCES.failed()],
    [{ code: "request", message: secret }, "failed", SENTENCES.failed()],
    [new Refusal("method", secret), "refusal", SENTENCES.method()],
    [new Refusal("busy"), "refusal", SENTENCES.busy()],
    [new Refusal("not_allowed", secret), "refusal", SENTENCES.not_allowed()],
    [new Refusal(secret), "failed", SENTENCES.failed()],
    [new Refusal("toString"), "failed", SENTENCES.failed()],
    [new Refusal("argument", secret), "refusal", SENTENCES.argument("of the request")],
    [new Refusal("argument", "onto_name"), "refusal", SENTENCES.argument("onto_name")],
    [new Failure("tabs", 3), "failed", "3 open tabs are at that URL, and exactly one is needed"],
    [new Failure("tabs", secret), "failed", "0 open tabs are at that URL, and exactly one is needed"],
    [new Failure("ended", secret), "failed", SENTENCES.ended()],
    [new Failure(secret), "failed", SENTENCES.failed()],
  ];
  for (const [error, kind, message] of thrown) {
    const handlers = {
      hover_element: async () => {
        throw error;
      },
    };
    const reply = await answer(request("hover_element", GOOD.hover_element), null, handlers);
    assert.deepEqual(reply, { v: 1, error: { kind, message } });
    assert.equal(JSON.stringify(reply).includes("SECRET"), false);
  }
  for (const name of ARGUMENTS) {
    assert.ok(SENTENCES.argument(name).includes(name));
  }
});

test("a second request while one runs is refused and starts nothing, and the first is not disturbed", { timeout: 5000 }, async () => {
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const started = [];
  const handlers = {
    hover_element: async (args) => {
      started.push(args.name);
      if (args.name === GOOD.hover_element.name) {
        await held;
      }
      return { status: "hovered", origin: ORIGIN_A };
    },
  };
  const first = answer(request("hover_element", GOOD.hover_element), null, handlers);
  const second = await answer(
    request("hover_element", { ...GOOD.hover_element, name: "Other" }),
    null,
    handlers,
  );
  assert.deepEqual(second, { v: 1, error: { kind: "refusal", message: SENTENCES.busy() } });
  assert.deepEqual(started, ["Docs"]);
  release();
  assert.deepEqual(await first, { v: 1, result: { status: "hovered", origin: ORIGIN_A } });
});

test("a request that ends, by a reply or by an error, lets the next one start", async () => {
  const handlers = {
    hover_element: async (args) => {
      if (args.name === "Fails") {
        throw new Error("the page went away");
      }
      return { status: "hovered", origin: ORIGIN_A };
    },
  };
  const failed = await answer(
    request("hover_element", { ...GOOD.hover_element, name: "Fails" }),
    null,
    handlers,
  );
  assert.equal(failed.error.kind, "failed");
  const next = await answer(request("hover_element", GOOD.hover_element), null, handlers);
  assert.equal(next.result.status, "hovered");
  const refused = await answer(request("nothing", {}), null, handlers);
  assert.equal(refused.error.kind, "refusal");
  assert.equal((await answer(request("hover_element", GOOD.hover_element), null, handlers)).result.status, "hovered");
});

test("each method of the table runs the tool of its name", () => {
  assert.deepEqual(Object.keys(HANDLERS), Object.keys(METHODS));
  assert.deepEqual(HANDLERS, TOOLS);
  assert.ok(Object.isFrozen(HANDLERS));
});

test("choose_file takes a url, a name, a path, a size and a file name, and a role where one is given, and no other argument", () => {
  const file = { url: URL_A, name: "Resume", path: "/srv/uploads/report.txt", size: 12, file_name: "report.txt" };
  assert.deepEqual({ ...parse(request("choose_file", file)).args }, file);
  assert.deepEqual({ ...parse(request("choose_file", { ...file, role: "button" })).args }, { ...file, role: "button" });
  // The role has no default, so a call without one holds none.
  assert.equal("role" in parse(request("choose_file", file)).args, false);
  for (const missing of ["url", "name", "path", "size", "file_name"]) {
    const { [missing]: left, ...rest } = file;
    assert.throws(
      () => parse(request("choose_file", rest)),
      (error) => error instanceof Refusal && error.code === "argument" && error.detail === missing,
      missing,
    );
  }
  for (const [argument, value] of [
    ["role", "listbox"],
    ["role", 7],
    ["name", ""],
    ["name", "n".repeat(words.NAME_LIMIT + 1)],
    ["path", ""],
    ["path", 7],
    ["size", "12"],
    ["size", 1.5],
    ["size", -1],
    ["file_name", ""],
    ["url", "file:///etc/passwd"],
  ]) {
    assert.throws(
      () => parse(request("choose_file", { ...file, [argument]: value })),
      (error) => error instanceof Refusal && error.code === "argument" && error.detail === argument,
      `${argument} ${value}`,
    );
  }
  // Nothing the method does not take: a node, a selector, a second file, a coordinate.
  for (const extra of ["nodeId", "backendNodeId", "selector", "files", "x", "onto_name", "text"]) {
    assert.throws(
      () => parse(request("choose_file", { ...file, [extra]: "x" })),
      (error) => error instanceof Refusal && error.code === "extra",
      extra,
    );
  }
  // The other methods do not take a path.
  assert.throws(
    () => parse(request("hover_element", { ...GOOD.hover_element, path: "/srv/uploads/a.txt" })),
    (error) => error instanceof Refusal && error.code === "extra",
  );
});

// files.js counts a path in code points and the relay does too, so a path that
// they accept has to be accepted here, whatever it holds outside the basic plane.
test("choose_file counts the path and the file name in code points, as files.js does", () => {
  const face = "\u{1F600}";
  const long = face.repeat(PATH_LIMIT - 1);
  const file = { url: URL_A, name: "Resume", path: `/${long}`, size: 12, file_name: long };
  assert.ok(file.path.length > PATH_LIMIT, "the path is not longer in UTF-16 units");
  assert.equal(problemWithFile(file), null);
  assert.deepEqual({ ...parse(request("choose_file", file)).args }, file);
  for (const [argument, value] of [
    ["path", `/${long}${face}`],
    ["file_name", face.repeat(PATH_LIMIT + 1)],
  ]) {
    assert.throws(
      () => parse(request("choose_file", { ...file, [argument]: value })),
      (error) => error instanceof Refusal && error.code === "argument" && error.detail === argument,
      argument,
    );
  }
});

test("the reply to choose_file is the status word chose and the origin, and nothing the request wrote", async () => {
  const { handlers } = recording("chose");
  const chose = await answer(
    request("choose_file", { url: URL_A, name: "Resume", path: "/srv/uploads/report.txt", size: 12, file_name: "report.txt" }),
    null,
    handlers,
  );
  assert.deepEqual(chose, { v: VERSION, result: { status: "chose", origin: ORIGIN_A } });
  assert.ok(STATUSES.includes("chose"));
  for (const word of ["Resume", "report.txt", "/srv"]) {
    assert.equal(JSON.stringify(chose).includes(word), false, word);
  }
});

test("parse gives the method and its arguments, or the refusal", () => {
  assert.deepEqual(parse(request("press_key_trusted", GOOD.press_key_trusted)).args, GOOD.press_key_trusted);
  assert.throws(
    () => parse(request("press_key_trusted", { url: URL_A })),
    (error) => error instanceof Refusal && error.code === "argument" && error.detail === "key",
  );
});

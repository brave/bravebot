// The one place the extension uses the browser's debugger, against a fake that
// records every call: what may be sent, what needs a token, how a session
// begins and ends, and which tab it is on.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import { approve } from "../approval.js";
import {
  ALLOWED,
  BEFORE_WINDOW_MS,
  BROWSER_WAIT_MS,
  CALL_WAIT_MS,
  DETACH_WAIT_MS,
  DIALOG_WAIT_MS,
  RELEASE_WAIT_MS,
  callTime,
  command,
  detachLeftovers,
  dialogOf,
  handleDialog,
  isWebUrl,
  owe,
  sendWatching,
  settle as settleOwed,
  withSession,
} from "../cdp.js";
import { Failure, Refusal, SENTENCES } from "../sentences.js";
import { callsOf, fakeDebugger, pageAt } from "./fake-debugger.mjs";
import { occurrences, scripts } from "./scripts.mjs";

const URL_A = "https://example.test/page";
const TOKEN = Object.freeze({ made: "by the approval" });
const allowsTheToken = (token) => token === TOKEN;

// The methods that only read the page, and the ones that send input or take a
// picture of it, which need a token.
const READS = [
  "Accessibility.enable",
  "Accessibility.getFullAXTree",
  "CSS.enable",
  "CSS.getComputedStyleForNode",
  "DOM.enable",
  "DOM.getBoxModel",
  "DOM.getFlattenedDocument",
  "DOM.getNodeForLocation",
  "DOM.scrollIntoViewIfNeeded",
  "Page.bringToFront",
  "Page.enable",
  "Page.getFrameTree",
  "Page.getLayoutMetrics",
];
// The ones that need a token: those that send input, the one that takes a picture
// of the page, and the two that start the report of the page's console.
const INPUTS = [
  "Console.enable",
  "DOM.focus",
  "DOM.setFileInputFiles",
  "Input.dispatchKeyEvent",
  "Input.dispatchMouseEvent",
  "Input.insertText",
  "Log.enable",
  "Page.captureScreenshot",
  "Page.handleJavaScriptDialog",
];
// The ones a session's approval allows with a token and sends with the parameters
// it is given. The answer to a dialog is sent with the person's answer, and the
// choice of a file only for one file and one node, which have their own tests below.
const WITH_A_TOKEN = INPUTS.filter(
  (method) => !["Page.handleJavaScriptDialog", "DOM.setFileInputFiles"].includes(method),
);

// A fake with one page at URL_A, and the result of a session on it that runs
// `work` with the approval given.
function onePage(options = {}) {
  return fakeDebugger({ targets: [pageAt(URL_A, 7)], ...options });
}

test("the list of methods is frozen, holds the methods that read, the ones that send input or take a picture, and none of Runtime", () => {
  assert.ok(Object.isFrozen(ALLOWED));
  assert.deepEqual(
    Object.keys(ALLOWED).filter((method) => !ALLOWED[method]),
    READS,
  );
  assert.deepEqual(
    Object.keys(ALLOWED).filter((method) => ALLOWED[method]),
    INPUTS,
  );
  assert.deepEqual(
    Object.fromEntries(INPUTS.map((method) => [method, ALLOWED[method]])),
    {
      "Console.enable": true,
      "DOM.focus": true,
      "DOM.setFileInputFiles": "file",
      "Input.dispatchKeyEvent": true,
      "Input.dispatchMouseEvent": true,
      "Input.insertText": true,
      "Log.enable": true,
      "Page.captureScreenshot": true,
      "Page.handleJavaScriptDialog": "dialog",
    },
  );
  assert.throws(() => {
    ALLOWED["Runtime.evaluate"] = false;
  }, TypeError);
  assert.equal(Object.hasOwn(ALLOWED, "Runtime.evaluate"), false);
  for (const method of Object.keys(ALLOWED)) {
    assert.doesNotMatch(
      method,
      /^(Runtime|Network|Storage|Fetch|Target|Browser|Emulation)\./,
      `${method} is on the list`,
    );
  }
  for (const method of [
    "Page.navigate",
    "Page.printToPDF",
    "Page.startScreencast",
    "Input.dispatchTouchEvent",
  ]) {
    assert.equal(Object.hasOwn(ALLOWED, method), false, `${method} is on the list`);
  }
});

test("a method that is not on the list is refused and never reaches the browser, with a token or without", async () => {
  const fake = onePage();
  await withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      for (const method of [
        "Runtime.evaluate",
        "Runtime.callFunctionOn",
        "Page.navigate",
        "Target.attachToTarget",
        "Network.getCookies",
        "Input.dispatchTouchEvent",
        "DOM.setFileInputFile",
        "DOM.setFileInputFiles ",
        "Page.printToPDF",
        "input.dispatchMouseEvent",
        "Input.dispatchMouseEvent ",
        "Input.",
        "Input",
        "toString",
        "constructor",
        "__proto__",
        "hasOwnProperty",
        undefined,
        null,
        5,
        {},
      ]) {
        await assert.rejects(
          command(session, method, {}, TOKEN),
          (error) => error instanceof Refusal && error.code === "method",
          `${String(method)} was not refused`,
        );
      }
    },
    { allows: () => true },
  );
  assert.deepEqual(callsOf(fake, "sendCommand"), []);
});

test("a method that sends input needs a token the approval accepts, and is sent once with it", async () => {
  for (const method of WITH_A_TOKEN) {
    const fake = onePage();
    await withSession(
      fake.chrome,
      URL_A,
      async (session) => {
        await command(session, "Page.enable");
        for (const token of [undefined, null, {}, "token", { ...TOKEN }]) {
          await assert.rejects(
            command(session, method, { x: 1 }, token),
            (error) => error instanceof Refusal && error.code === "not_allowed",
            `${method} was sent without the token`,
          );
        }
        assert.deepEqual(callsOf(fake, "sendCommand").map((call) => call.method), ["Page.enable"]);
        await command(session, method, { x: 1 }, TOKEN);
      },
      { allows: allowsTheToken },
    );
    assert.deepEqual(callsOf(fake, "sendCommand").at(-1), {
      call: "sendCommand",
      tabId: 7,
      method,
      params: { x: 1 },
    });
    assert.equal(callsOf(fake, "sendCommand").length, 2);
  }
});

// The choice of a file is the one input whose parameters name something outside the
// page: a path the browser reads. It needs the person's token like any input, and it
// is sent only for one file whose path files.js accepts and for one node, so a caller
// cannot have the browser read a second file, a relative path, a path through `..` or
// a credential, or send a parameter this code never read.
test("a file is chosen only with the token, for one path files.js accepts and one node, and nothing else is sent to the browser", async () => {
  const good = { files: ["/srv/uploads/report.txt"], backendNodeId: 12 };
  const bad = [
    {},
    { files: ["/srv/uploads/report.txt"] },
    { backendNodeId: 12 },
    { ...good, nodeId: 3 },
    { ...good, objectId: "x" },
    { ...good, backendNodeId: "12" },
    { ...good, backendNodeId: 1.5 },
    { ...good, files: "/srv/uploads/report.txt" },
    { ...good, files: [] },
    { ...good, files: ["/srv/uploads/report.txt", "/srv/uploads/second.txt"] },
    { ...good, files: [7] },
    { ...good, files: ["report.txt"] },
    { ...good, files: ["/srv/uploads/../secret.txt"] },
    { ...good, files: ["/srv/uploads/./report.txt"] },
    { ...good, files: ["/srv//uploads/report.txt"] },
    { ...good, files: ["/srv/uploads/"] },
    { ...good, files: ["/srv/uploads/report.txt\n"] },
    { ...good, files: ["/srv/uploads/.env"] },
    { ...good, files: ["/srv/uploads/server.pem"] },
    { ...good, files: ["/srv/.ssh/known_hosts"] },
    { ...good, files: [`/${"a".repeat(900)}`] },
    undefined,
    null,
  ];
  const fake = onePage();
  await withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      for (const token of [undefined, null, {}, "token", { ...TOKEN }]) {
        await assert.rejects(
          command(session, "DOM.setFileInputFiles", good, token),
          (error) => error instanceof Refusal && error.code === "not_allowed",
        );
      }
      for (const params of bad) {
        await assert.rejects(
          command(session, "DOM.setFileInputFiles", params, TOKEN),
          (error) => error instanceof Refusal && error.code === "argument",
          JSON.stringify(params),
        );
      }
      assert.deepEqual(callsOf(fake, "sendCommand").map((call) => call.method), ["Page.enable"]);
      await command(session, "DOM.setFileInputFiles", good, TOKEN);
    },
    { allows: allowsTheToken },
  );
  assert.deepEqual(callsOf(fake, "sendCommand").at(-1), {
    call: "sendCommand",
    tabId: 7,
    method: "DOM.setFileInputFiles",
    params: good,
  });
  assert.equal(callsOf(fake, "sendCommand").length, 2);
});

test("no input is sent before Page is enabled, with a token or without, and every input is once it is", async () => {
  for (const method of WITH_A_TOKEN) {
    const fake = onePage();
    await withSession(
      fake.chrome,
      URL_A,
      async (session) => {
        await assert.rejects(
          command(session, method, {}, TOKEN),
          (error) => error instanceof Failure && error.code === "failed",
          `${method} was sent before Page.enable`,
        );
        await command(session, "Accessibility.enable");
        await assert.rejects(command(session, method, {}, TOKEN), Failure);
        assert.deepEqual(
          callsOf(fake, "sendCommand").map((call) => call.method),
          ["Accessibility.enable"],
        );
        await command(session, "Page.enable");
        await command(session, method, {}, TOKEN);
      },
      { allows: allowsTheToken },
    );
  }
  // Page.enable that failed has not enabled it.
  const failing = onePage({ failOn: { sendCommand: (method) => method === "Page.enable" && new Error("no") } });
  await withSession(
    failing.chrome,
    URL_A,
    async (session) => {
      await assert.rejects(command(session, "Page.enable"), /no/);
      await assert.rejects(command(session, "Input.insertText", {}, TOKEN), Failure);
    },
    { allows: allowsTheToken },
  );
  assert.deepEqual(callsOf(failing, "sendCommand").map((call) => call.method), ["Page.enable"]);
});

test("with no approval given, no method that sends input is sent, and one that only reads is", async () => {
  const fake = onePage();
  await withSession(fake.chrome, URL_A, async (session) => {
    for (const method of INPUTS) {
      await assert.rejects(
        command(session, method, {}, TOKEN),
        (error) => error instanceof Refusal && error.code === "not_allowed",
      );
    }
    for (const method of READS) {
      await command(session, method, {});
    }
  });
  assert.deepEqual(
    callsOf(fake, "sendCommand").map((call) => call.method),
    READS,
  );
});

test("an approval that answers with anything but true does not allow", async () => {
  const fake = onePage();
  const answers = [Promise.resolve(false), Promise.resolve(true), "yes", 1, {}];
  for (const answer of answers) {
    await withSession(
      fake.chrome,
      URL_A,
      async (session) => {
        await assert.rejects(
          command(session, "Input.insertText", { text: "a" }, TOKEN),
          (error) => error instanceof Refusal,
        );
      },
      { allows: () => answer },
    );
  }
  assert.deepEqual(callsOf(fake, "sendCommand"), []);
});

test("a method that only reads is sent without a token, to the tab, and its answer is returned", async () => {
  const fake = onePage({
    answers: { "Page.getLayoutMetrics": () => ({ cssContentSize: { width: 3 } }) },
  });
  const answer = await withSession(fake.chrome, URL_A, (session) =>
    command(session, "Page.getLayoutMetrics", { deep: true }),
  );
  assert.deepEqual(answer, { cssContentSize: { width: 3 } });
  assert.deepEqual(callsOf(fake, "sendCommand"), [
    { call: "sendCommand", tabId: 7, method: "Page.getLayoutMetrics", params: { deep: true } },
  ]);
});

test("a session attaches to the tab at version 1.3, runs the work, detaches, and returns what the work returned", async () => {
  const fake = onePage();
  const result = await withSession(fake.chrome, URL_A, async (session) => {
    await command(session, "DOM.enable", {});
    assert.equal(fake.attached.has(7), true, "not attached while the work runs");
    return "done";
  });
  assert.equal(result, "done");
  assert.deepEqual(fake.calls, [
    { call: "getTargets" },
    { call: "attach", tabId: 7, version: "1.3" },
    { call: "sendCommand", tabId: 7, method: "DOM.enable", params: {} },
    { call: "detach", tabId: 7 },
  ]);
  assert.equal(fake.attached.size, 0);
});

test("a session is detached when a command throws, when the work throws, and the error is the work's even if detaching fails too", async () => {
  const broken = new Error("the page went away");
  const fake = onePage({
    failOn: { sendCommand: (method) => (method === "DOM.enable" ? broken : undefined) },
  });
  await assert.rejects(
    withSession(fake.chrome, URL_A, (session) => command(session, "DOM.enable", {})),
    (error) => error === broken,
  );
  assert.equal(callsOf(fake, "detach").length, 1);
  assert.equal(fake.attached.size, 0);

  const work = new Error("the work failed");
  const second = onePage();
  await assert.rejects(
    withSession(second.chrome, URL_A, async () => {
      throw work;
    }),
    (error) => error === work,
  );
  assert.equal(callsOf(second, "detach").length, 1);
  assert.equal(second.attached.size, 0);

  const third = onePage({ failOn: { detach: new Error("detach failed") } });
  await assert.rejects(
    withSession(third.chrome, URL_A, async () => {
      throw work;
    }),
    (error) => error === work,
  );
  assert.equal(callsOf(third, "detach").length, 1);
  assert.equal(await withSession(third.chrome, URL_A, async () => "kept"), "kept");
});

test("a failed attach runs no work and detaches nothing", async () => {
  const refused = new Error("Another debugger is already attached to the tab with id: 7.");
  const fake = onePage({ failOn: { attach: refused } });
  let ran = false;
  await assert.rejects(
    withSession(fake.chrome, URL_A, async () => {
      ran = true;
    }),
    (error) => error === refused,
  );
  assert.equal(ran, false);
  assert.deepEqual(callsOf(fake, "detach"), []);
});

test("a session that has ended, or that this file did not make, sends nothing", async () => {
  const fake = onePage();
  let kept;
  await withSession(fake.chrome, URL_A, async (session) => {
    kept = session;
  });
  await assert.rejects(
    command(kept, "DOM.enable", {}),
    (error) => error instanceof Failure && error.code === "ended",
  );
  const attached = onePage();
  await withSession(attached.chrome, URL_A, async () => {
    await assert.rejects(
      command({ tabId: 7 }, "DOM.enable", {}),
      (error) => error instanceof Failure && error.code === "ended",
    );
  });
  assert.deepEqual(callsOf(fake, "sendCommand"), []);
  assert.deepEqual(callsOf(attached, "sendCommand"), []);
});

test("a session holds only the number of its tab", async () => {
  const fake = onePage();
  await withSession(fake.chrome, URL_A, async (session) => {
    assert.deepEqual(Object.keys(session), ["tabId"]);
    assert.equal(session.tabId, 7);
    assert.ok(Object.isFrozen(session));
  });
});

test("the tab is the one page at the address, character for character", async () => {
  const fake = fakeDebugger({
    targets: [
      { ...pageAt(URL_A, 1), type: "service_worker" },
      { ...pageAt(URL_A, 2), type: "iframe" },
      { ...pageAt(URL_A, 3), type: "background_page" },
      { ...pageAt(URL_A, undefined), tabId: undefined },
      pageAt(`${URL_A}/`, 4),
      pageAt(`${URL_A}#part`, 5),
      pageAt(URL_A.toUpperCase(), 6),
      pageAt("https://example.test/pagex", 8),
      pageAt(URL_A, 9),
    ],
  });
  await withSession(fake.chrome, URL_A, async (session) => {
    assert.equal(session.tabId, 9);
  });
  assert.deepEqual(callsOf(fake, "attach"), [{ call: "attach", tabId: 9, version: "1.3" }]);
});

test("no tab, or several, at the address is a failure that says how many and nothing else, and attaches nowhere", async () => {
  const secret = "SECRET-TITLE-4431";
  const titled = (url, tabId) => pageAt(url, tabId, secret);
  for (const [targets, count] of [
    [[], 0],
    [[titled("https://other.test/", 1), titled(`${URL_A}/`, 2)], 0],
    [[titled(URL_A, 1), titled(URL_A, 2)], 2],
    [[titled(URL_A, 1), titled(URL_A, 2), titled(URL_A, 3)], 3],
  ]) {
    const fake = fakeDebugger({ targets });
    let caught;
    try {
      await withSession(fake.chrome, URL_A, async () => "ran");
    } catch (error) {
      caught = error;
    }
    assert.ok(caught instanceof Failure && caught.code === "tabs");
    assert.equal(SENTENCES.tabs(caught.detail), SENTENCES.tabs(count));
    const shown = JSON.stringify([caught.message, caught.code, caught.detail]);
    for (const hidden of [secret, "other.test", "example.test"]) {
      assert.equal(shown.includes(hidden), false, `the failure shows ${hidden}`);
    }
    assert.deepEqual(callsOf(fake, "attach"), []);
  }
});

test("an address that is not a page on the web is refused before the browser is asked", async () => {
  for (const url of [
    "chrome://version",
    "brave://settings",
    "file:///etc/passwd",
    "about:blank",
    "javascript:alert(1)",
    "data:text/html,x",
    "chrome-extension://abc/page.html",
    "example.test/page",
    "",
    undefined,
    null,
    7,
  ]) {
    const fake = fakeDebugger({ targets: [pageAt(String(url), 1)] });
    await assert.rejects(
      withSession(fake.chrome, url, async () => "ran"),
      (error) => error instanceof Refusal && error.code === "argument",
      `${String(url)} was not refused`,
    );
    assert.deepEqual(fake.calls, []);
  }
  assert.equal(isWebUrl("http://example.test/"), true);
  assert.equal(isWebUrl("https://example.test:8443/a?b#c"), true);
});

// How many times each pattern appears in the code of the extension outside its
// tests, by file, comments left out.
const CODE = scripts();

test("the browser's debugger is named once in the extension, in cdp.js, and a method is sent from one place", () => {
  // actions.js has the word twice, in the words the window has for a picture and
  // for a console. The pattern of the next line finds the use of the API.
  assert.deepEqual(occurrences(CODE, "\\bdebugger\\b"), { "actions.js": 2, "cdp.js": 1 });
  assert.deepEqual(occurrences(CODE, "chrome\\s*\\.\\s*debugger"), { "cdp.js": 1 });
  assert.deepEqual(occurrences(CODE, "sendCommand"), { "cdp.js": 1 });
  // The scan finds a second use, in this file or another, and one that is
  // written another way.
  const second = [{ name: "other.js", code: ["const d = chrome.debugger;"] }];
  assert.deepEqual(occurrences([...CODE, ...second], "\\bdebugger\\b"), {
    "actions.js": 2,
    "cdp.js": 1,
    "other.js": 1,
  });
  for (const sample of [
    'const { debugger: d } = chrome;',
    'chrome["debugger"].attach(a);',
    "chrome . debugger.attach(a);",
    "debugger;",
  ]) {
    assert.deepEqual(
      occurrences([{ name: "x.js", code: [sample] }], "\\bdebugger\\b"),
      { "x.js": 1 },
      sample,
    );
  }
});

test("no script runs code from a string, reaches the network, stores or logs, or names a Runtime method", () => {
  // The patterns read the source as written, so a Runtime method named by joining
  // parts is not found by them. The list of methods in cdp.js, which is frozen and
  // holds none (the test of that list, above), is what refuses it.
  const forbidden = [
    "\\beval\\b",
    "new\\s+Function",
    "\\bimportScripts\\b",
    "\\bfetch\\s*\\(",
    "XMLHttpRequest",
    "WebSocket",
    "sendBeacon",
    "console\\s*\\.",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "chrome\\s*\\.\\s*storage",
    "chrome\\s*\\.\\s*tabs",
    "chrome\\s*\\.\\s*scripting",
    "chrome\\s*\\.\\s*runtime\\s*\\.\\s*sendMessage",
    "\\bRuntime\\.",
  ];
  for (const pattern of forbidden) {
    assert.deepEqual(occurrences(CODE, pattern), {}, `a script matches ${pattern}`);
  }
  // Only the two windows' own pages open a connection to the worker.
  assert.deepEqual(occurrences(CODE, "chrome\\s*\\.\\s*runtime\\s*\\.\\s*connect"), {
    "confirm.js": 1,
    "dialog.js": 1,
  });
  // The scan reads the files it is meant to, and finds each pattern in a sample.
  assert.ok(CODE.some(({ name }) => name === "cdp.js"));
  assert.ok(CODE.length >= 6, `${CODE.length} scripts were read`);
  for (const sample of [
    "eval(x)",
    "new Function(x)",
    "fetch(url)",
    "console.log(t)",
    "chrome.storage.local.set(a)",
    "chrome.runtime.sendMessage(i, m)",
    "sendCommand({ tabId }, 'Runtime.evaluate', p)",
  ]) {
    assert.ok(
      forbidden.some(
        (pattern) => occurrences([{ name: "x.js", code: [sample] }], pattern)["x.js"],
      ),
      `${sample} is not found`,
    );
  }
});

// A dialog that a page opens: the event the browser sends, with fields the
// session does not read.
const OPENING = {
  url: "https://example.test/page",
  message: "Delete everything?",
  type: "confirm",
  hasBrowserHandler: false,
  defaultPrompt: "",
};

// A session on tab 7 with Page enabled, for the tests of dialogs. `held` is the
// promise a mouse release waits on while a dialog is open, and `fake.answered`
// lists the answers the page was given.
async function withDialogSession(work, options = {}) {
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const answered = [];
  const fake = onePage({
    answers: {
      "Input.dispatchMouseEvent": (params) => (params.type === "mouseReleased" ? held : {}),
      "Page.handleJavaScriptDialog": (params) => {
        answered.push(params);
        release({});
        return {};
      },
    },
    ...options,
  });
  fake.answered = answered;
  fake.release = release;
  const result = await withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      return work(session, fake);
    },
    { allows: () => true },
  );
  return { fake, result };
}

// The answer the dialog panel gives for `dialog`: a token made for the
// resolution that holds the dialog.
function answerFor(dialog, { accept = true, text = "" } = {}) {
  const resolution = Object.freeze({ dialog, description: Object.freeze({ type: dialog.type }) });
  return Object.freeze({ resolution, token: approve(resolution), accept, text });
}

test("the session sees the dialog a page opens as its kind, its words and its suggested answer and nothing else, and only for its own tab", async () => {
  const { result } = await withDialogSession(async (session, fake) => {
    assert.equal(dialogOf(session), null);
    fake.emit("Page.javascriptDialogOpening", OPENING, 8);
    fake.emit("Page.javascriptDialogClosed", {}, 7);
    fake.emit("Page.frameNavigated", OPENING, 7);
    assert.equal(dialogOf(session), null, "another tab's dialog or another event was taken");
    fake.emit("Page.javascriptDialogOpening", OPENING, 7);
    return dialogOf(session);
  });
  assert.deepEqual(result, { type: "confirm", message: "Delete everything?", defaultPrompt: "" });
  assert.ok(Object.isFrozen(result));
  const { result: odd } = await withDialogSession(async (session, fake) => {
    fake.emit("Page.javascriptDialogOpening", { type: 5, message: { a: 1 } }, 7);
    return dialogOf(session);
  });
  assert.deepEqual(odd, { type: "", message: "", defaultPrompt: "" });
  assert.equal(dialogOf({ tabId: 7 }), null, "a session this file did not make has a dialog");
});

test("the listener for the browser's events is added for the session and removed when it ends, however it ends", async () => {
  const fake = onePage();
  await withSession(fake.chrome, URL_A, async () => {
    assert.equal(fake.watchers.size, 1);
  });
  assert.equal(fake.watchers.size, 0);
  await assert.rejects(
    withSession(fake.chrome, URL_A, async () => {
      throw new Error("work failed");
    }),
    /work failed/,
  );
  assert.equal(fake.watchers.size, 0);
  const failing = onePage({ failOn: { attach: new Error("no") } });
  await assert.rejects(withSession(failing.chrome, URL_A, async () => {}), /no/);
  assert.equal(failing.watchers.size, 0);
});

beforeEach(() => mock.timers.enable({ apis: ["setTimeout"] }));
afterEach(() => mock.timers.reset());

const settle = async () => {
  for (let turn = 0; turn < 20; turn += 1) {
    await Promise.resolve();
  }
};

const MOUSE_UP = { type: "mouseReleased", x: 1, y: 2, button: "left", clickCount: 1 };

test("input that opens a dialog is not waited for past the dialog: the dialog is returned while the browser has not answered, and the answer follows the person's", async () => {
  const { result } = await withDialogSession(async (session, fake) => {
    let returned = null;
    const sending = sendWatching(session, "Input.dispatchMouseEvent", MOUSE_UP, TOKEN).then((value) => {
      returned = value;
    });
    await settle();
    assert.equal(returned, null, "returned before a dialog opened or the time passed");
    fake.emit("Page.javascriptDialogOpening", OPENING, 7);
    await sending;
    assert.deepEqual(returned.dialog, { type: "confirm", message: "Delete everything?", defaultPrompt: "" });
    let finished = false;
    returned.done.then(() => {
      finished = true;
    });
    await settle();
    assert.equal(finished, false, "the browser answered while the dialog was open");
    await handleDialog(session, answerFor(returned.dialog, { accept: false }));
    await returned.done;
    assert.deepEqual(fake.answered, [{ accept: false }]);
    return dialogOf(session);
  });
  assert.equal(result, null, "the dialog is still open after it was answered");
});

test("input the browser answers returns at once with no dialog, one that nothing answers returns after the wait, and a refused input rejects", async () => {
  assert.equal(DIALOG_WAIT_MS, 2000);
  await withDialogSession(async (session) => {
    const quick = await sendWatching(
      session,
      "Input.dispatchMouseEvent",
      { ...MOUSE_UP, type: "mousePressed" },
      TOKEN,
    );
    assert.equal(quick.dialog, null);
    await quick.done;
  });
  await withDialogSession(async (session, fake) => {
    let returned = null;
    const sending = sendWatching(session, "Input.dispatchMouseEvent", MOUSE_UP, TOKEN).then((value) => {
      returned = value;
    });
    await settle();
    mock.timers.tick(DIALOG_WAIT_MS - 1);
    await settle();
    assert.equal(returned, null, "returned before the wait was over");
    mock.timers.tick(1);
    await sending;
    assert.equal(returned.dialog, null);
    fake.release({});
    await returned.done;
  });
  await withDialogSession(async (session, fake) => {
    const sending = sendWatching(session, "Input.insertText", { text: "a" }, TOKEN, 10);
    await settle();
    mock.timers.tick(10);
    await sending;
    fake.release({});
  });
  const fake = onePage();
  await assert.rejects(
    withSession(fake.chrome, URL_A, async (session) => {
      await command(session, "Page.enable");
      await sendWatching(session, "Input.insertText", { text: "a" }, TOKEN);
    }),
    (error) => error instanceof Refusal && error.code === "not_allowed",
  );
  assert.equal(callsOf(fake, "sendCommand").some((call) => call.method === "Input.insertText"), false);
  await assert.rejects(sendWatching({ tabId: 7 }, "Input.insertText", {}, TOKEN), Failure);
});

test("input sent while a dialog is open is refused and sends nothing", async () => {
  await withDialogSession(async (session, fake) => {
    fake.emit("Page.javascriptDialogOpening", OPENING, 7);
    const sent = callsOf(fake, "sendCommand").length;
    await assert.rejects(
      sendWatching(session, "Input.dispatchMouseEvent", MOUSE_UP, TOKEN),
      (error) => error instanceof Failure && error.code === "dialog_open",
    );
    assert.equal(callsOf(fake, "sendCommand").length, sent);
  });
});

test("a dialog is answered with the person's answer alone: the accept flag, and for a prompt the text they typed, whatever the caller passes", async () => {
  const cases = [
    [{ type: "confirm" }, { accept: true, text: "ignored" }, { accept: true }],
    [{ type: "confirm" }, { accept: false }, { accept: false }],
    [{ type: "alert" }, { accept: true }, { accept: true }],
    [{ type: "beforeunload" }, { accept: false }, { accept: false }],
    [{ type: "prompt" }, { accept: true, text: "Ada" }, { accept: true, promptText: "Ada" }],
    [{ type: "prompt" }, { accept: true, text: "" }, { accept: true, promptText: "" }],
    [{ type: "prompt" }, { accept: false, text: "Ada" }, { accept: false }],
  ];
  for (const [kind, given, sent] of cases) {
    const { fake } = await withDialogSession(async (session, fake) => {
      fake.emit("Page.javascriptDialogOpening", { ...OPENING, ...kind }, 7);
      const dialog = dialogOf(session);
      // The caller's own parameters are not read.
      await command(session, "Page.handleJavaScriptDialog", { accept: !given.accept, promptText: "x" }, answerFor(dialog, given));
    });
    assert.deepEqual(fake.answered, [sent], JSON.stringify([kind, given]));
  }
});

test("a dialog is not answered with anything but the person's answer for the dialog that is open, once", async () => {
  await withDialogSession(async (session, fake) => {
    const refused = async (answer, why, answers = 0) => {
      await assert.rejects(
        handleDialog(session, answer),
        (error) => error instanceof Refusal && error.code === "not_allowed",
        why,
      );
      assert.equal(fake.answered.length, answers, `${why} reached the page`);
    };
    // No dialog is open.
    await refused(answerFor({ type: "alert", message: "", defaultPrompt: "" }), "an answer with no dialog open");
    fake.emit("Page.javascriptDialogOpening", { ...OPENING, type: "prompt" }, 7);
    const dialog = dialogOf(session);
    const real = answerFor(dialog, { accept: true, text: "Ada" });
    await refused(undefined, "no answer");
    await refused(null, "a null answer");
    await refused({ ...real, token: { ...real.token } }, "a token this extension did not make");
    await refused({ ...real, token: TOKEN }, "the token of an action");
    await refused({ ...answerFor({ ...dialog }), accept: true, text: "" }, "an answer for another dialog object");
    // A token for the action's resolution, which holds no dialog.
    const action = Object.freeze({ description: { action: "click" } });
    await refused({ resolution: action, token: approve(action), accept: true, text: "" }, "a token for an action");
    await refused({ ...real, accept: "yes" }, "an accept flag that is not a boolean");
    await refused({ ...real, text: 5 }, "a prompt answered with a number");
    await refused({ ...real, text: "t".repeat(2001) }, "a prompt answered with too long a text");
    // The refusals above spent nothing that belongs to the real answer.
    await handleDialog(session, real);
    assert.deepEqual(fake.answered, [{ accept: true, promptText: "Ada" }]);
    await refused(real, "the same answer twice", 1);
  });
});

test("the token of an answer to a dialog is spent by the answer that uses it, and not by a refusal that comes before the token is looked at", async () => {
  await withDialogSession(async (session, fake) => {
    fake.emit("Page.javascriptDialogOpening", OPENING, 7);
    const dialog = dialogOf(session);
    const real = answerFor(dialog);
    await assert.rejects(handleDialog(session, { ...real, accept: "yes" }), Refusal);
    await handleDialog(session, real);
    await assert.rejects(handleDialog(session, real), Refusal);
    assert.equal(fake.answered.length, 1);
  });
});

test("an answer to a dialog that the browser refuses is the browser's error, and the dialog stays recorded as open", async () => {
  await withDialogSession(
    async (session, fake) => {
      fake.emit("Page.javascriptDialogOpening", OPENING, 7);
      await assert.rejects(handleDialog(session, answerFor(dialogOf(session))), /No dialog is showing/);
      assert.notEqual(dialogOf(session), null);
    },
    {
      failOn: {
        sendCommand: (method) =>
          method === "Page.handleJavaScriptDialog" && new Error("No dialog is showing"),
      },
    },
  );
});

test("the answer to a dialog is named in cdp.js alone, and a token is made in the window code alone", () => {
  assert.deepEqual(occurrences(CODE, "Page\\.handleJavaScriptDialog"), { "cdp.js": 2 });
  assert.deepEqual(occurrences(CODE, "\\bapprove\\s*\\("), {
    "approval.js": 1,
    "confirmation.js": 1,
  });
  // A token is spent by the approval of a call (tools.js) and by the answer to a
  // dialog.
  assert.deepEqual(occurrences(CODE, "\\bspend\\s*\\("), {
    "approval.js": 1,
    "cdp.js": 1,
    "tools.js": 1,
  });
});

// Starts `start`, lets what it awaits settle, passes `ms` of the clock and says
// how it ended.
async function endedAfter(start, ms) {
  const ended = start().then(
    () => "returned",
    (error) => error,
  );
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  mock.timers.tick(ms);
  return ended;
}

test("a read, the list of tabs, an attach and a detach that the browser never answers fail after the wait, and the session still ends", async () => {
  const never = () => new Promise(() => {});
  const failed = (error) => error instanceof Failure && error.code === "failed";
  const read = onePage({ answers: { "Page.getLayoutMetrics": never } });
  const reading = await endedAfter(
    () => withSession(read.chrome, URL_A, (session) => command(session, "Page.getLayoutMetrics")),
    BROWSER_WAIT_MS,
  );
  assert.ok(failed(reading), "a read that is never answered held the request");
  assert.equal(callsOf(read, "detach").length, 1);
  const targets = onePage();
  targets.chrome.debugger.getTargets = never;
  assert.ok(failed(await endedAfter(() => withSession(targets.chrome, URL_A, async () => {}), BROWSER_WAIT_MS)));
  const attach = onePage();
  attach.chrome.debugger.attach = never;
  assert.ok(failed(await endedAfter(() => withSession(attach.chrome, URL_A, async () => {}), BROWSER_WAIT_MS)));
  const detach = onePage();
  detach.chrome.debugger.detach = never;
  assert.equal(
    await endedAfter(() => withSession(detach.chrome, URL_A, async () => {}), BROWSER_WAIT_MS),
    "returned",
  );
});

test("a detach the browser does not answer holds the end of a call for DETACH_WAIT_MS, which is shorter than the wait of a read, and no longer", async () => {
  assert.ok(DETACH_WAIT_MS < BROWSER_WAIT_MS);
  const fake = onePage();
  fake.chrome.debugger.detach = () => new Promise(() => {});
  let ended = false;
  const running = withSession(fake.chrome, URL_A, async () => {}).then(() => {
    ended = true;
  });
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  mock.timers.tick(DETACH_WAIT_MS - 1);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(ended, false, "the call ended before the wait for the detach");
  mock.timers.tick(1);
  await running;
  assert.equal(ended, true);
});

test("an attach the browser answers after the call gave up on it is detached, so a tab is not left with a debugger no call owns", async () => {
  const fake = onePage();
  const attach = fake.chrome.debugger.attach;
  let land;
  fake.chrome.debugger.attach = (target, version) =>
    new Promise((resolve) => {
      land = () => attach(target, version).then(resolve);
    });
  const failure = await endedAfter(() => withSession(fake.chrome, URL_A, async () => {}), BROWSER_WAIT_MS);
  assert.ok(failure instanceof Failure && failure.code === "failed");
  assert.equal(callsOf(fake, "detach").length, 0);
  land();
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  assert.equal(callsOf(fake, "detach").length, 1);
  assert.equal(fake.attached.size, 0, "the debugger stays attached to the tab");
});

test("a call that ran out of time ends its session at that moment, while the detach still waits for the browser, so work that goes on cannot send", async () => {
  const fake = onePage({ answers: { "Page.enable": () => ({}) } });
  fake.chrome.debugger.detach = () => new Promise(() => {});
  const time = callTime();
  time.start();
  let held = null;
  const running = withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      held = session;
      await new Promise(() => {});
    },
    { time },
  );
  const settled = running.then(
    () => "returned",
    (error) => error.code,
  );
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  await command(held, "Page.enable");
  mock.timers.tick(CALL_WAIT_MS);
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  // The detach has not been answered, and the call has not returned.
  await assert.rejects(command(held, "Page.enable"), (error) => error instanceof Failure && error.code === "ended");
  mock.timers.tick(DETACH_WAIT_MS);
  assert.equal(await settled, "late");
});

test("the time after the person's answer counts CALL_WAIT_MS from the last start, and not before the first or while it is stopped", async () => {
  const time = callTime();
  const never = new Promise(() => {});
  const outcome = (promise) => {
    const state = { value: "pending" };
    time.within(promise).then(
      (value) => (state.value = `settled ${value}`),
      (error) => (state.value = `${error.code}`),
    );
    return state;
  };
  const turn = () => new Promise((resolve) => setImmediate(resolve));
  const waiting = outcome(never);
  mock.timers.tick(CALL_WAIT_MS * 2);
  await turn();
  assert.equal(waiting.value, "pending", "it counted before the person answered");

  time.start();
  mock.timers.tick(CALL_WAIT_MS - 1);
  time.start();
  mock.timers.tick(CALL_WAIT_MS - 1);
  await turn();
  assert.equal(waiting.value, "pending", "a start did not count again from its own moment");
  time.stop();
  mock.timers.tick(CALL_WAIT_MS * 2);
  await turn();
  assert.equal(waiting.value, "pending", "it counted while it was stopped");
  time.start();
  mock.timers.tick(CALL_WAIT_MS - 1);
  await turn();
  assert.equal(waiting.value, "pending");
  mock.timers.tick(1);
  await turn();
  assert.equal(waiting.value, "late");
});

test("a promise that settles first is what the time after the answer returns, a failure of it included, and a promise it abandoned reports nothing later", async () => {
  const time = callTime();
  time.start();
  assert.equal(await time.within(Promise.resolve("a")), "a");
  const refusal = new Refusal("denied");
  await assert.rejects(time.within(Promise.reject(refusal)), (error) => error === refusal);
  let rejectLater;
  const abandoned = new Promise((_, reject) => {
    rejectLater = reject;
  });
  const waiting = time.within(abandoned).catch((error) => error.code);
  mock.timers.tick(CALL_WAIT_MS);
  assert.equal(await waiting, "late");
  rejectLater(new Error("after"));
  await new Promise((resolve) => setImmediate(resolve));
});

test("input the browser does not answer is not held to the wait of a read, since a dialog it opens keeps the answer back", async () => {
  const fake = onePage({
    answers: {
      "Page.enable": () => ({}),
      "Input.dispatchMouseEvent": () => new Promise(() => {}),
    },
  });
  let opened = "not known";
  const running = withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      await command(session, "Page.enable");
      const sent = await sendWatching(session, "Input.dispatchMouseEvent", {}, TOKEN);
      opened = sent.dialog === null ? "no dialog" : "dialog";
      let state = "pending";
      sent.done.then(
        () => (state = "answered"),
        () => (state = "failed"),
      );
      mock.timers.tick(BROWSER_WAIT_MS * 2);
      await new Promise((resolve) => setImmediate(resolve));
      assert.equal(state, "pending", "input was given up on after the wait of a read");
    },
    { allows: allowsTheToken },
  );
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  mock.timers.tick(DIALOG_WAIT_MS);
  await running;
  assert.equal(opened, "no dialog");
});

test("the time of a call counts BEFORE_WINDOW_MS from begin() with the failure slow, counts CALL_WAIT_MS from start() with late instead of it, and counts nothing while stopped", async () => {
  const turn = () => new Promise((resolve) => setImmediate(resolve));
  const outcome = (time) => {
    const state = { value: "pending" };
    time.within(new Promise(() => {})).then(
      () => (state.value = "settled"),
      (error) => (state.value = error.code),
    );
    return state;
  };
  // Before the window: slow, at BEFORE_WINDOW_MS and not before.
  const first = callTime();
  const before = outcome(first);
  mock.timers.tick(BEFORE_WINDOW_MS * 4);
  await turn();
  assert.equal(before.value, "pending", "it counted before begin()");
  first.begin();
  mock.timers.tick(BEFORE_WINDOW_MS - 1);
  await turn();
  assert.equal(before.value, "pending");
  mock.timers.tick(1);
  await turn();
  assert.equal(before.value, "slow");
  // The person is asked: nothing counts, and the answer counts the time after it.
  const second = callTime();
  const asked = outcome(second);
  second.begin();
  mock.timers.tick(BEFORE_WINDOW_MS - 1);
  second.stop();
  mock.timers.tick(CALL_WAIT_MS * 3);
  await turn();
  assert.equal(asked.value, "pending", "it counted while the person was asked");
  second.start();
  mock.timers.tick(BEFORE_WINDOW_MS * 2);
  await turn();
  assert.equal(asked.value, "pending", "the time before the window went on after the answer");
  mock.timers.tick(CALL_WAIT_MS - BEFORE_WINDOW_MS * 2);
  await turn();
  assert.equal(asked.value, "late");
  // The time before the window counts again from begin().
  const third = callTime();
  const again = outcome(third);
  third.begin();
  mock.timers.tick(BEFORE_WINDOW_MS - 1);
  third.begin();
  mock.timers.tick(BEFORE_WINDOW_MS - 1);
  await turn();
  assert.equal(again.value, "pending", "begin() did not count again from its own moment");
  mock.timers.tick(1);
  await turn();
  assert.equal(again.value, "slow");
});

// A call whose work presses and then does `end`, with the input it owes recorded
// before the press. `end` is "throws", "returns" or "never" (the call's time
// passes), and `after` runs before the call ends, with the session.
async function endsOwing({ end, fake = onePage({ answers: { "Page.enable": () => ({}) } }), after = () => {}, owed = RELEASE }) {
  const time = callTime();
  let session = null;
  const running = withSession(
    fake.chrome,
    URL_A,
    async (held) => {
      session = held;
      await command(held, "Page.enable");
      time.start();
      owe(held, owed.method, owed.params, TOKEN);
      await command(held, owed.method, { type: "mousePressed" }, TOKEN);
      await after(held);
      if (end === "throws") {
        throw new Failure("changed");
      }
      if (end === "never") {
        await new Promise(() => {});
      }
    },
    { allows: allowsTheToken, time },
  );
  const settled = running.then(
    () => "returned",
    (error) => error.code ?? String(error),
  );
  for (let turn = 0; turn < 8; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  if (end === "never") {
    mock.timers.tick(CALL_WAIT_MS);
  }
  const how = await settled;
  return { how, fake, session };
}
const RELEASE = {
  method: "Input.dispatchMouseEvent",
  params: { type: "mouseReleased", x: 3, y: 4, button: "left", buttons: 0, clickCount: 1 },
};
const sentOf = (fake) => callsOf(fake, "sendCommand").filter((entry) => entry.method.startsWith("Input."));

test("a call that ends with an input still owed sends it once, before it detaches, whether the work throws, returns or runs out of time", async () => {
  for (const [end, expected] of [["throws", "changed"], ["returns", "returned"], ["never", "late"]]) {
    const { how, fake } = await endsOwing({ end });
    assert.equal(how, expected, end);
    const inputs = sentOf(fake);
    assert.deepEqual(
      inputs.map((entry) => entry.params.type),
      ["mousePressed", "mouseReleased"],
      `${end}: the press is released once`,
    );
    assert.deepEqual(inputs[1].params, RELEASE.params, end);
    const order = fake.calls.map((entry) => entry.call);
    assert.ok(order.lastIndexOf("sendCommand") < order.indexOf("detach"), `${end}: the release goes before the detach: ${order.join()}`);
    assert.equal(callsOf(fake, "detach").length, 1, end);
  }
});

test("an input that was settled is not sent at the end of the call, and an input recorded later replaces an earlier one", async () => {
  const settled = await endsOwing({ end: "never", after: (session) => settleOwed(session) });
  assert.equal(settled.how, "late");
  assert.deepEqual(
    sentOf(settled.fake).map((entry) => entry.params.type),
    ["mousePressed"],
    "a release that was settled was sent",
  );
  const replaced = await endsOwing({
    end: "returns",
    after: (session) => owe(session, "Input.dispatchKeyEvent", { type: "keyUp", key: "Enter" }, TOKEN),
  });
  assert.deepEqual(
    sentOf(replaced.fake).map((entry) => `${entry.method} ${entry.params.type}`),
    ["Input.dispatchMouseEvent mousePressed", "Input.dispatchKeyEvent keyUp"],
  );
});

test("a page with a dialog open gets no release at the end of the call, since the browser answers no input and would apply it when the person answers the dialog", async () => {
  const fake = onePage({ answers: { "Page.enable": () => ({}) } });
  const { how } = await endsOwing({
    end: "never",
    fake,
    after: () => fake.emit("Page.javascriptDialogOpening", { type: "confirm", message: "Sure?", defaultPrompt: "" }, 7),
  });
  assert.equal(how, "late");
  assert.deepEqual(sentOf(fake).map((entry) => entry.params.type), ["mousePressed"]);
  assert.equal(callsOf(fake, "detach").length, 1);
});

test("the release at the end of a call waits RELEASE_WAIT_MS for the browser and the detach after it, the two together no longer than DETACH_WAIT_MS", async () => {
  const fake = onePage({ answers: { "Page.enable": () => ({}) } });
  const api = fake.chrome.debugger;
  const sendCommand = api.sendCommand;
  api.sendCommand = (target, method, params) =>
    params?.type === "mouseReleased" ? (sendCommand(target, method, params), new Promise(() => {})) : sendCommand(target, method, params);
  api.detach = () => new Promise(() => {});
  const ended = { value: false };
  const running = endsOwing({ end: "never", fake }).then(() => (ended.value = true));
  for (let turn = 0; turn < 10; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  // The deadline passed inside endsOwing(); the release and the detach are now waiting.
  mock.timers.tick(RELEASE_WAIT_MS);
  for (let turn = 0; turn < 5; turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  assert.equal(ended.value, false, "the call ended before the wait for the detach");
  mock.timers.tick(DETACH_WAIT_MS - RELEASE_WAIT_MS);
  await running;
  assert.equal(ended.value, true);
  assert.deepEqual(sentOf(fake).map((entry) => entry.params.type), ["mousePressed", "mouseReleased"]);
});

test("an input is recorded as owed only if it could be sent: on the list, one that sends input, with the token of the approval, in a session that has not ended", async () => {
  const fake = onePage({ answers: { "Page.enable": () => ({}) } });
  let ended = null;
  await withSession(
    fake.chrome,
    URL_A,
    async (session) => {
      ended = session;
      await command(session, "Page.enable");
      for (const [method, token, code] of [
        ["Runtime.evaluate", TOKEN, "method"],
        ["Page.enable", TOKEN, "method"],
        ["Page.getLayoutMetrics", TOKEN, "method"],
        ["Page.handleJavaScriptDialog", TOKEN, "method"],
        ["Input.dispatchMouseEvent", undefined, "not_allowed"],
        ["Input.dispatchMouseEvent", { made: "elsewhere" }, "not_allowed"],
      ]) {
        assert.throws(() => owe(session, method, {}, token), (error) => error instanceof Refusal && error.code === code, `${method} ${JSON.stringify(token)}`);
      }
    },
    { allows: allowsTheToken },
  );
  assert.deepEqual(sentOf(fake), [], "a refused input was sent at the end");
  assert.throws(() => owe(ended, RELEASE.method, RELEASE.params, TOKEN), (error) => error instanceof Failure && error.code === "ended");
  assert.doesNotThrow(() => settleOwed(ended));
});

test("work that goes on after the call ran out of time cannot record an input or send one", async () => {
  const fake = onePage({ answers: { "Page.enable": () => ({}) } });
  const { session } = await endsOwing({ end: "never", fake });
  const before = fake.calls.length;
  assert.throws(() => owe(session, RELEASE.method, RELEASE.params, TOKEN), (error) => error.code === "ended");
  await assert.rejects(command(session, RELEASE.method, RELEASE.params, TOKEN), (error) => error.code === "ended");
  assert.equal(fake.calls.length, before);
});

test("leftover sessions are detached for every tab that lists a debugger attached and no other, a failure of one does not stop the others, and a call waits for it only for the time before the window", async () => {
  const fake = fakeDebugger({
    targets: [
      { ...pageAt(URL_A, 7), attached: true },
      pageAt("https://example.test/other", 8),
      { ...pageAt("https://example.test/third", 9), attached: true },
      { type: "service_worker", id: "w", url: "chrome-extension://x/", attached: true },
      { ...pageAt("https://example.test/fourth", 10), attached: true },
      { ...pageAt("https://example.test/frame", 11), type: "iframe", attached: true },
    ],
  });
  fake.attached.add(7);
  fake.attached.add(10);
  fake.attached.add(11);
  await detachLeftovers(fake.chrome);
  assert.deepEqual(
    callsOf(fake, "detach").map((entry) => entry.tabId),
    [7, 9, 10],
    "a session of the worker that stopped was not detached, or one that nobody listed was",
  );
  assert.deepEqual([...fake.attached], [11], "a target that is not a page was detached");
  // The listing never answers: a call that begins counting at once ends with slow, and attaches to nothing.
  const stuck = onePage();
  stuck.chrome.debugger.getTargets = () => new Promise(() => {});
  detachLeftovers(stuck.chrome);
  const time = callTime();
  time.begin();
  const failure = await endedAfter(() => withSession(stuck.chrome, URL_A, async () => {}, { time }), BEFORE_WINDOW_MS);
  assert.ok(failure instanceof Failure && failure.code === "slow", String(failure?.code));
  assert.deepEqual(callsOf(stuck, "attach"), []);
  // The cleaning up of a worker that answers again is what later calls wait for.
  await detachLeftovers(onePage().chrome);
});

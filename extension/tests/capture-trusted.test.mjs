// The tool of the capture-trusted group, which this extension checks and sends to
// the BraveBot Trusted Input extension, and whose reply is a picture: what has to
// hold before anything is sent, what is sent, what shape of reply is passed on as
// a picture, and that every other tool's reply is still a word. Run once against
// the other extension's own code, over a fake debugger.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { activityLog } from "../activity.js";
import { NO_ACCESS } from "../groups.js";
import { RUNS_KEY } from "../lifetimes.js";
import { MAX_PICTURE_BYTES, PICTURE_TYPE, decodedSize, isBase64 } from "../pictures.js";
import { setTool, turnOn } from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  BUSY_MESSAGE,
  GROUP_SWITCHES_KEY,
  MAX_PICTURE_BYTES as TOOLS_CAP,
  SITE_SCOPES_KEY,
  handle,
} from "../tools.js";
import {
  NOT_THERE,
  NOT_UNDERSTOOD,
  PICTURE_METHODS,
  SECOND_ID,
  STOPPED,
  TOO_LATE,
  VERSION,
  WAIT_MS,
  forward,
} from "../trusted.js";
import { approve, setConfirmation, CONFIRMATION_WAIT_MS } from "../../extension-trusted/approval.js";
import { CAPTURE_WAIT_MS, MAX_PICTURE_BYTES as SECONDS_CAP } from "../../extension-trusted/capture.js";
import { answer } from "../../extension-trusted/requests.js";
import { fakeDebugger, pageAt } from "../../extension-trusted/tests/fake-debugger.mjs";
import { fakeChrome } from "./fake-chrome.mjs";

afterEach(() => {
  mock.timers.reset();
  setConfirmation();
});

const NEWS = "https://news.example";
const PAGE = `${NEWS}/front`;
const BANK = "https://bank.example";
const METHOD = "screenshot_full_page";
const CALL = { url: PAGE };

// The base64 of a JPEG header of `width` by `height` pixels, then zeros up to
// `size` bytes. A reply to the tool is the picture of the page the other
// extension took.
function jpeg(width, height, size = 0) {
  const head = Buffer.from([
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00,
    0x01, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, height >> 8, height & 0xff, width >> 8, width & 0xff,
    0x01, 0x01, 0x11, 0x00,
  ]);
  const fill = Math.max(0, size - head.length - 2);
  return Buffer.concat([head, Buffer.alloc(fill, 9), Buffer.from([0xff, 0xd9])]).toString("base64");
}

const PICTURE = jpeg(433, 2047);
const picture = (data = PICTURE, more = {}) => ({
  v: VERSION,
  result: { image: { mimeType: PICTURE_TYPE, data, ...more } },
});

// A browser in which the person turned capture-trusted on and allowed the sites
// in `allowed`, and in which the other extension answers each message with
// `reply`: a reply, or a function of the id sent to and the request that gives
// one or throws. `sent` lists every message that was sent.
async function trusted({ allowed = [NEWS], blocked = [], reply = picture(), local = {} } = {}) {
  const chrome = fakeChrome({
    held: {},
    local: { [ALLOWED_SITES_KEY]: allowed, [BLOCKED_SITES_KEY]: blocked, ...local },
    tabs: [{ id: 1, windowId: 1, title: "News", url: PAGE }],
  });
  chrome.sent = [];
  chrome.runtime.sendMessage = async (id, request) => {
    chrome.sent.push([id, request]);
    return typeof reply === "function" ? reply(id, request) : reply;
  };
  assert.equal(await turnOn(chrome, "capture-trusted", NO_ACCESS), true);
  return chrome;
}

const call = (chrome, method, params) => handle({ id: 7, method, params }, chrome);

// Lets what is waiting on a resolved promise run.
async function settle() {
  for (let turn = 0; turn < 50; turn += 1) {
    await Promise.resolve();
  }
}

test("the cap, the media type and the picture methods are the same in both extensions and the client's", () => {
  assert.equal(MAX_PICTURE_BYTES, 8 * 1024 * 1024);
  assert.equal(TOOLS_CAP, MAX_PICTURE_BYTES);
  assert.equal(SECONDS_CAP, MAX_PICTURE_BYTES);
  assert.equal(PICTURE_TYPE, "image/jpeg");
  assert.deepEqual([...PICTURE_METHODS], [METHOD]);
});

test("the work after the person's answer fits in what is left of the wait for the other extension, with room before it", () => {
  assert.ok(CONFIRMATION_WAIT_MS + CAPTURE_WAIT_MS < WAIT_MS);
  assert.ok(WAIT_MS - CONFIRMATION_WAIT_MS - CAPTURE_WAIT_MS >= 5_000);
});

test("a call sends the address and no other argument, to the other extension, and returns its picture and nothing else", async () => {
  const chrome = await trusted();
  const reply = await call(chrome, METHOD, { ...CALL, selector: "#a", x: 1, y: 2, tab_id: 1, scale: 3, format: "png", role: "button" });
  assert.equal(reply.error, undefined, reply.error?.message);
  assert.deepEqual(chrome.sent, [[SECOND_ID, { v: VERSION, method: METHOD, params: { url: PAGE } }]]);
  assert.deepEqual(reply.result, { image: { mimeType: "image/jpeg", data: PICTURE } });
  assert.deepEqual(Object.keys(reply.result), ["image"]);
});

// The checks come first. A forward that came before any of them would send a
// request for a group that is off, a tool that is off, a group whose time has
// ended, a site nobody allowed or a site that is blocked.
test("nothing is sent while the group is off, the tool is off, the lifetime has ended, the site is not allowed or the site is blocked", async () => {
  const off = await trusted();
  off.local[GROUP_SWITCHES_KEY] = {};
  assert.match((await call(off, METHOD, CALL)).error.message, /in the capture-trusted group, which is turned off/);
  assert.deepEqual(off.sent, []);

  const toolOff = await trusted();
  await setTool(toolOff, METHOD, false);
  assert.match((await call(toolOff, METHOD, CALL)).error.message, /is turned off in the BraveBot extension's options/);
  assert.deepEqual(toolOff.sent, []);

  const lapsed = await trusted();
  lapsed.session[RUNS_KEY] = {};
  assert.match((await call(lapsed, METHOD, CALL)).error.message, /was turned on until Brave restarts, and that has ended/);
  assert.deepEqual(lapsed.sent, []);

  const elsewhere = await trusted({ allowed: [BANK] });
  assert.match((await call(elsewhere, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(elsewhere.sent, []);

  const blocked = await trusted({ allowed: [NEWS], blocked: [NEWS] });
  assert.match((await call(blocked, METHOD, CALL)).error.message, /in the blocked sites list/);
  assert.deepEqual(blocked.sent, []);

  for (const url of [`blob:${NEWS}/0b1c2d3e`, `view-source:${PAGE}`, "about:blank", "", undefined]) {
    const chrome = await trusted({ allowed: [NEWS, "about://blank"] });
    assert.ok((await call(chrome, METHOD, { url })).error, String(url));
    assert.deepEqual(chrome.sent, [], String(url));
  }
});

test("the group is limited to the allowed sites whatever scope is stored, and an allowed list that cannot be read allows none", async () => {
  const widened = await trusted({
    allowed: [BANK],
    local: { [SITE_SCOPES_KEY]: { "capture-trusted": "all", "page-read": "all" } },
  });
  assert.match((await call(widened, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(widened.sent, []);

  const unreadable = await trusted({ allowed: "https://news.example" });
  assert.match((await call(unreadable, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(unreadable.sent, []);
});

// A picture is passed on only when it is the one shape a picture has. Each of
// these replies is a picture a wrong check would pass: a type that is not on the
// list, a field the picture does not have, base64 that is not canonical, and a
// size a byte over.
test("a reply is passed on as a picture only when it is a JPEG as canonical base64 within the cap, in an object that holds the picture and nothing else", async () => {
  const over = jpeg(433, 2047, MAX_PICTURE_BYTES + 1);
  const exact = jpeg(433, 2047, MAX_PICTURE_BYTES);
  const cases = {
    "a PNG": { v: VERSION, result: { image: { mimeType: "image/png", data: PICTURE } } },
    "an SVG": { v: VERSION, result: { image: { mimeType: "image/svg+xml", data: PICTURE } } },
    "a type with a different case": { v: VERSION, result: { image: { mimeType: "image/JPEG", data: PICTURE } } },
    "no type": { v: VERSION, result: { image: { data: PICTURE } } },
    "no data": { v: VERSION, result: { image: { mimeType: PICTURE_TYPE } } },
    "a field beside the picture's": picture(PICTURE, { caption: "Secret" }),
    "a field beside the image": { v: VERSION, result: { image: { mimeType: PICTURE_TYPE, data: PICTURE }, origin: NEWS } },
    "a status in place of an image": { v: VERSION, result: { status: "clicked", origin: NEWS } },
    "a status beside the image": { v: VERSION, result: { image: { mimeType: PICTURE_TYPE, data: PICTURE }, status: "clicked" } },
    "an image that is a string": { v: VERSION, result: { image: PICTURE } },
    "an image that is a list": { v: VERSION, result: { image: [PICTURE_TYPE, PICTURE] } },
    "data that is not a string": picture(12345),
    "empty data": picture(""),
    "data with a space": picture(`${PICTURE.slice(0, 8)} ${PICTURE.slice(8)}`),
    "data with a line end": picture(`${PICTURE}\n`),
    "data of the url-safe alphabet": picture("-_-_"),
    "data of a length that is not a multiple of four": picture(PICTURE.slice(0, -1)),
    "data with padding in the middle": picture("QQ==QQ=="),
    "data a byte over the cap": picture(over),
    "an empty result": { v: VERSION, result: {} },
    "a result that is a string": { v: VERSION, result: PICTURE },
    "a result with an error beside it": { v: VERSION, result: { image: { mimeType: PICTURE_TYPE, data: PICTURE } }, error: { kind: "failed", message: "x" } },
    "a version that is not ours": { v: VERSION + 1, result: { image: { mimeType: PICTURE_TYPE, data: PICTURE } } },
  };
  for (const [label, reply] of Object.entries(cases)) {
    const chrome = await trusted({ reply });
    const got = await call(chrome, METHOD, CALL);
    assert.equal(got.error?.message, NOT_UNDERSTOOD, label);
    assert.equal(got.result, undefined, label);
    assert.equal(JSON.stringify(got).includes(PICTURE.slice(0, 20)), false, `${label}: the picture was passed on`);
  }
  // The cap is exact, and a picture is passed on as it is.
  const chrome = await trusted({ reply: picture(exact) });
  const taken = await call(chrome, METHOD, CALL);
  assert.equal(decodedSize(taken.result.image.data), MAX_PICTURE_BYTES);
  assert.equal(taken.result.image.data, exact);
});

test("the tool's own table and no reply says which tool's reply is a picture: a picture for any other tool, and a status for the picture tool, are not understood", async () => {
  for (const method of ["click_element_trusted", "hover_element"]) {
    const chrome = await trusted({ reply: picture() });
    const reply = await forward(chrome, method, { url: PAGE, role: "link", name: "Docs" }).catch((error) => error);
    assert.equal(reply.message, NOT_UNDERSTOOD, method);
  }
  const status = await trusted({ reply: { v: VERSION, result: { status: "clicked", origin: NEWS } } });
  const reply = await forward(status, METHOD, CALL).catch((error) => error);
  assert.equal(reply.message, NOT_UNDERSTOOD);
  // The ordinary tools still answer a status word and the origin.
  const ordinary = await trusted({ reply: { v: VERSION, result: { status: "hovered", origin: NEWS } } });
  assert.deepEqual(await forward(ordinary, "hover_element", { url: PAGE, role: "link", name: "Docs" }), {
    status: "hovered",
    origin: NEWS,
  });
});

test("an error of the other extension is a refusal or a failure with its sentence after our own, for the picture tool as for the others", async () => {
  const refused = await trusted({
    reply: { v: VERSION, error: { kind: "refusal", message: "it was denied in the confirmation window, so nothing was done" } },
  });
  const denied = await call(refused, METHOD, CALL);
  assert.equal(
    denied.error.message,
    "the BraveBot Trusted Input extension refused it: it was denied in the confirmation window, so nothing was done",
  );
  assert.equal((await activityLog(refused)).at(-1).outcome, "refused");

  const failed = await trusted({
    reply: { v: VERSION, error: { kind: "failed", message: "the picture is over 8 MiB, which is the most a picture may be, so none was returned" } },
  });
  const big = await call(failed, METHOD, CALL);
  assert.match(big.error.message, /^the BraveBot Trusted Input extension could not do it: the picture is over 8 MiB/);
  assert.equal((await activityLog(failed)).at(-1).outcome, "failed");
});

test("an extension that is not there is one sentence of refusal, a worker that stopped is a failure, and a reply that does not come ends the call at the wait", async () => {
  const absent = await trusted({
    reply: () => {
      throw new Error("Could not establish connection. Receiving end does not exist.");
    },
  });
  const refused = await call(absent, METHOD, CALL);
  assert.equal(refused.error.message, NOT_THERE);
  assert.equal((await activityLog(absent)).at(-1).outcome, "refused");

  const stopped = await trusted({
    reply: () => {
      throw new Error("The message port closed before a response was received.");
    },
  });
  assert.equal((await call(stopped, METHOD, CALL)).error.message, STOPPED);

  mock.timers.enable({ apis: ["setTimeout"] });
  const silent = await trusted();
  silent.runtime.sendMessage = () => new Promise(() => {});
  let outcome;
  forward(silent, METHOD, CALL).then(
    () => (outcome = "done"),
    (error) => (outcome = error),
  );
  await settle();
  mock.timers.tick(WAIT_MS - 1);
  await settle();
  assert.equal(outcome, undefined);
  mock.timers.tick(1);
  await settle();
  assert.equal(outcome.message, TOO_LATE);
});

test("while the picture is taken no other action is started, and the place is free again however the call ended", async () => {
  const chrome = await trusted();
  let release;
  chrome.runtime.sendMessage = (id, request) => {
    chrome.sent.push([id, request]);
    return new Promise((resolve) => {
      release = resolve;
    });
  };
  const first = call(chrome, METHOD, CALL);
  await settle();
  assert.equal(chrome.sent.length, 1);
  assert.equal((await call(chrome, METHOD, CALL)).error.message, BUSY_MESSAGE);
  assert.equal(chrome.sent.length, 1, "a second request was sent");
  release(picture());
  assert.ok((await first).result.image);
  chrome.runtime.sendMessage = async () => {
    throw new Error("The message port closed before a response was received.");
  };
  assert.equal((await call(chrome, METHOD, CALL)).error.message, STOPPED);
  chrome.runtime.sendMessage = async () => picture();
  assert.ok((await call(chrome, METHOD, CALL)).result.image);
});

test("the activity log keeps the group, the tool, the outcome and the origin and nothing of the picture", async () => {
  const chrome = await trusted();
  await call(chrome, METHOD, CALL);
  const log = await activityLog(chrome);
  assert.deepEqual(
    log.map(({ group, tool, outcome, origins }) => ({ group, tool, outcome, origins })),
    [{ group: "capture-trusted", tool: METHOD, outcome: "ran", origins: [NEWS] }],
  );
  assert.equal(JSON.stringify(log).includes(PICTURE.slice(0, 20)), false);
  assert.equal(JSON.stringify(log).includes("/front"), false);
});

test("canonical base64 and the size it decodes to", () => {
  assert.equal(isBase64("QQ=="), true);
  assert.equal(isBase64("QUJD"), true);
  for (const bad of [undefined, null, 5, "", "====", "QQ=", "QU-_", "QU JD", "QUJD\n", "QUJ", "=QUJ", "QQ==QQ=="]) {
    assert.equal(isBase64(bad), false, JSON.stringify(bad));
  }
  assert.equal(decodedSize("QQ=="), 1);
  assert.equal(decodedSize("QUI="), 2);
  assert.equal(decodedSize("QUJD"), 3);
});

// The whole chain in this process: this extension's call, the other extension's
// request check, tool and capture, over a fake debugger, and the window answered
// as the person would. What arrives is the picture the debugger gave.
test("a call through the other extension's own code returns the picture the browser took, and a denial returns a refusal and takes none", async () => {
  const taken = [];
  const shot = jpeg(433, 2047, 5000);
  const debug = fakeDebugger({
    targets: [pageAt(PAGE, 1)],
    answers: {
      "Page.getFrameTree": () => ({ frameTree: { frame: { url: PAGE } } }),
      "Page.getLayoutMetrics": () => ({
        cssContentSize: { width: 1280, height: 6040 },
        contentSize: { width: 1280, height: 6040 },
      }),
      "Page.captureScreenshot": (params) => {
        taken.push(params);
        return { data: shot };
      },
    },
  });
  const chrome = await trusted({ reply: (id, request) => answer(request, debug.chrome) });
  const asked = [];
  setConfirmation(async (resolution) => {
    asked.push(resolution.description);
    return approve(resolution);
  });
  const reply = await call(chrome, METHOD, CALL);
  assert.deepEqual(reply.result, { image: { mimeType: "image/jpeg", data: shot } });
  assert.equal(taken.length, 1);
  assert.equal(asked.length, 1);
  assert.equal(asked[0].action, "picture");
  assert.equal(asked[0].size, "1280 by 6040 CSS pixels");

  setConfirmation(async () => null);
  const denied = await call(chrome, METHOD, CALL);
  assert.match(denied.error.message, /refused it: it was denied in the confirmation window, so nothing was done$/);
  assert.equal(taken.length, 1, "a picture was taken after a denial");
  assert.equal(debug.attached.size, 0);
});

// The tool screenshot_full_page against a fake debugger that records every call,
// in order, and a stand-in for the window the person answers in. The tests ask
// through requests.js, as the BraveBot extension does, so what they compare is
// the reply and what the browser was sent. The picture's header is read from
// bytes made here and from bytes a real Brave made.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { approve, setConfirmation } from "../approval.js";
import {
  CAPTURE_WAIT_MS,
  LONG_SIDE,
  MAX_PICTURE_BYTES,
  PICTURE_QUALITY,
  PICTURE_TYPE,
  isPicture,
  jpegSize,
  pictureBytes,
} from "../capture.js";
import { ALLOWED, command, withSession } from "../cdp.js";
import { PICTURES, answer } from "../requests.js";
import { Refusal, SENTENCES } from "../sentences.js";
import { callsOf, fakeDebugger, pageAt } from "./fake-debugger.mjs";
import { occurrences, scripts } from "./scripts.mjs";

afterEach(() => {
  setConfirmation();
  mock.timers.reset();
});

const URL_A = "https://example.test/page";
const ORIGIN = "https://example.test";
const METHOD = "screenshot_full_page";
const SHOT = "Page.captureScreenshot";

// The base64 of a JPEG that has the header of one `width` by `height` pixels: the
// start of image, a JFIF segment, an ICC profile segment of `profile` bytes, the
// frame header, then zeros up to `size` bytes in all and the end of image. The
// frame header is progressive when `progressive` is true.
function jpeg(width, height, { profile = 0, progressive = false, size = 0 } = {}) {
  const parts = [
    Buffer.from([0xff, 0xd8]),
    Buffer.from([0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]),
  ];
  if (profile > 0) {
    parts.push(Buffer.from([0xff, 0xe2, (profile + 2) >> 8, (profile + 2) & 0xff]), Buffer.alloc(profile, 7));
  }
  parts.push(
    Buffer.from([
      0xff,
      progressive ? 0xc2 : 0xc0,
      0x00,
      0x0b,
      0x08,
      height >> 8,
      height & 0xff,
      width >> 8,
      width & 0xff,
      0x01,
      0x01,
      0x11,
      0x00,
    ]),
  );
  const head = Buffer.concat(parts);
  const fill = Math.max(0, size - head.length - 2);
  return Buffer.concat([head, Buffer.alloc(fill, 9), Buffer.from([0xff, 0xd9])]).toString("base64");
}

// The start of two pictures Brave 1.98.62 made with Page.captureScreenshot in format
// JPEG, quality 80: the first 642 bytes of one of 434 by 2048 pixels, whose colour profile
// puts the frame header at byte 632, and the first 168 bytes of one of 2048 by 226 pixels, whose
// frame header is at byte 158.
const BRAVE_TALL = "/9j/4AAQSkZJRgABAQAAAQABAAD/4gHYSUNDX1BST0ZJTEUAAQEAAAHIAAAAAAQwAABtbnRyUkdCIFhZWiAH4AABAAEAAAAAAABhY3NwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAA9tYAAQAAAADTLQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAlkZXNjAAAA8AAAACRyWFlaAAABFAAAABRnWFlaAAABKAAAABRiWFlaAAABPAAAABR3dHB0AAABUAAAABRyVFJDAAABZAAAAChnVFJDAAABZAAAAChiVFJDAAABZAAAAChjcHJ0AAABjAAAADxtbHVjAAAAAAAAAAEAAAAMZW5VUwAAAAgAAAAcAHMAUgBHAEJYWVogAAAAAAAAb6IAADj1AAADkFhZWiAAAAAAAABimQAAt4UAABjaWFlaIAAAAAAAACSgAAAPhAAAts9YWVogAAAAAAAA9tYAAQAAAADTLXBhcmEAAAAAAAQAAAACZmYAAPKnAAANWQAAE9AAAApbAAAAAAAAAABtbHVjAAAAAAAAAAEAAAAMZW5VUwAAACAAAAAcAEcAbwBvAGcAbABlACAASQBuAGMALgAgADIAMAAxADb/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAgAAbID";
const BRAVE_WIDE = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCADiCAAD";

// A request of the tool for the page.
const request = (params = {}) => ({ v: 1, method: METHOD, params: { url: URL_A, ...params } });

// The reply of a tool that was refused or failed, with the sentence of the table.
const refused = (code, detail) => ({ v: 1, error: { kind: "refusal", message: SENTENCES[code](detail) } });
const failed = (code, detail) => ({ v: 1, error: { kind: "failed", message: SENTENCES[code](detail) } });

// A page, a debugger over it and a stand-in for the window. `page` is what the
// browser reports of the page's content: its size in CSS pixels, and the pixels
// of the same size on the screen. `shots` is the data the browser answers
// captureScreenshot with, as a function from its parameters, or a list used in
// order. `allow` is true to allow, false to answer nothing, or a function from
// the resolution to what the window answers.
function rig({
  page = { width: 1280, height: 6040, factor: 1 },
  shots = (params) => jpeg(Math.floor(page.width * params.clip.scale * page.factor), Math.floor(page.height * params.clip.scale * page.factor)),
  allow = true,
  urls = [URL_A],
  metrics,
  failOn,
  taken,
} = {}) {
  const asked = [];
  const answers = {
    "Page.getFrameTree": () => ({ frameTree: { frame: { url: urls[Math.min(asked.length ? 1 : 0, urls.length - 1)] } } }),
    "Page.getLayoutMetrics": () =>
      metrics ?? {
        cssContentSize: { width: page.width, height: page.height },
        contentSize: { width: page.width * page.factor, height: page.height * page.factor },
      },
    [SHOT]: taken ?? ((params) => ({ data: Array.isArray(shots) ? shots.shift() : shots(params) })),
  };
  const fake = fakeDebugger({ targets: [pageAt(URL_A, 7)], answers, failOn });
  const windows = [];
  setConfirmation(async (resolution) => {
    asked.push(resolution);
    windows.push({
      description: resolution.description,
      calls: fake.calls.map((entry) => entry.method ?? entry.call),
    });
    return typeof allow === "function" ? allow(resolution) : allow ? approve(resolution) : null;
  });
  return { fake, windows, page };
}

// Lets every promise that can settle without a timer settle.
const settle = () => new Promise((resolve) => setImmediate(resolve));

const shotsOf = (fake) => callsOf(fake, "sendCommand").filter((entry) => entry.method === SHOT);
const detaches = (fake) => callsOf(fake, "detach").length;

test("a picture of the whole page is taken as a JPEG of quality 80 of a clip of the page's content from its top left corner, beyond the viewport, after the window, and the reply is the picture and nothing else", async () => {
  const r = rig();
  const reply = await answer(request(), r.fake.chrome);
  const [taken] = shotsOf(r.fake);
  assert.deepEqual(taken.params, {
    format: "jpeg",
    quality: 80,
    captureBeyondViewport: true,
    fromSurface: true,
    clip: { x: 0, y: 0, width: 1280, height: 6040, scale: 0.339 },
  });
  assert.equal(shotsOf(r.fake).length, 1);
  assert.deepEqual(Object.keys(reply), ["v", "result"]);
  assert.deepEqual(Object.keys(reply.result), ["image"]);
  assert.deepEqual(Object.keys(reply.result.image).sort(), ["data", "mimeType"]);
  assert.equal(reply.result.image.mimeType, "image/jpeg");
  assert.deepEqual(jpegSize(reply.result.image.data), { width: 433, height: 2047 });
  assert.equal(PICTURE_QUALITY, 80);
  assert.equal(PICTURE_TYPE, "image/jpeg");
  assert.equal(LONG_SIDE, 2048);
});

test("the person is asked after the page is brought to the front and before anything is captured, and is shown the site, the address and the size of the page and no element", async () => {
  const r = rig();
  await answer(request(), r.fake.chrome);
  assert.equal(r.windows.length, 1);
  const [{ description, calls }] = r.windows;
  assert.deepEqual(description, {
    action: "picture",
    origin: ORIGIN,
    url: URL_A,
    tag: "",
    role: null,
    name: "",
    excerpt: "",
    goes: "",
    size: "1280 by 6040 CSS pixels",
  });
  assert.ok(calls.includes("Page.bringToFront"));
  assert.ok(calls.includes("Page.getLayoutMetrics"));
  assert.ok(!calls.includes(SHOT), "a picture was taken before the person was asked");
  const order = r.fake.calls.map((entry) => entry.method ?? entry.call);
  assert.ok(order.indexOf("Page.bringToFront") < order.indexOf(SHOT));
  assert.ok(order.indexOf("Page.enable") < order.indexOf(SHOT));
});

test("nothing is captured when the person denies, when the window answers nothing, or when it answers with a token made for another question, and each ends with the debugger detached", async () => {
  const cases = {
    "the person denies": { allow: () => Promise.reject(new Refusal("denied")), reply: refused("denied") },
    "the window answers nothing": { allow: false, reply: refused("denied") },
    "the window answers something that is not a token": { allow: () => "yes", reply: refused("denied") },
    "the token was made for another resolution": {
      allow: () => approve(Object.freeze({ description: {} })),
      reply: refused("not_allowed"),
    },
  };
  for (const [label, { allow, reply }] of Object.entries(cases)) {
    const r = rig({ allow });
    assert.deepEqual(await answer(request(), r.fake.chrome), reply, label);
    assert.deepEqual(shotsOf(r.fake), [], label);
    assert.equal(detaches(r.fake), 1, label);
  }
});

test("the scale makes the long side 2048 pixels or less, never enlarges, counts the pixels the screen makes of a CSS pixel, and rounds down", async () => {
  const cases = [
    // [page, the scale asked for]
    [{ width: 1280, height: 6040, factor: 1 }, 0.339],
    [{ width: 1280, height: 719, factor: 1 }, 1],
    [{ width: 6000, height: 719, factor: 1 }, 0.3413],
    [{ width: 1280, height: 40000, factor: 1 }, 0.0512],
    [{ width: 1280, height: 6040, factor: 2 }, 0.1695],
    [{ width: 1000, height: 1000, factor: 1 }, 1],
    [{ width: 2048, height: 100, factor: 1 }, 1],
    [{ width: 2049, height: 100, factor: 1 }, 0.9995],
  ];
  for (const [page, scale] of cases) {
    const r = rig({ page });
    const reply = await answer(request(), r.fake.chrome);
    assert.equal(shotsOf(r.fake)[0].params.clip.scale, scale, JSON.stringify(page));
    const { width, height } = jpegSize(reply.result.image.data);
    assert.ok(Math.max(width, height) <= LONG_SIDE, `${width} by ${height} for ${JSON.stringify(page)}`);
  }
});

test("a picture the browser made larger than the scale meant is taken again at the scale its size gives, and a second one that is still too large is refused with no picture", async () => {
  // The browser reports a factor of 1 on a screen that makes two pixels of one.
  const page = { width: 1280, height: 6040, factor: 1 };
  const r = rig({
    page,
    taken: (params) => ({
      data: jpeg(Math.floor(page.width * params.clip.scale * 2), Math.floor(page.height * params.clip.scale * 2)),
    }),
  });
  const reply = await answer(request(), r.fake.chrome);
  const scales = shotsOf(r.fake).map((entry) => entry.params.clip.scale);
  assert.deepEqual(scales, [0.339, 0.1695]);
  const { width, height } = jpegSize(reply.result.image.data);
  assert.ok(Math.max(width, height) <= LONG_SIDE, `${width} by ${height}`);
  assert.equal(detaches(r.fake), 1);

  const stubborn = rig({ taken: () => ({ data: jpeg(3000, 4000) }) });
  assert.deepEqual(await answer(request(), stubborn.fake.chrome), failed("picture_size"));
  assert.equal(shotsOf(stubborn.fake).length, 2, "at most two pictures are taken");
  assert.equal(detaches(stubborn.fake), 1);
});

test("a picture of exactly 8 MiB is returned and one a byte over is refused with the fixed sentence and nothing of it", async () => {
  const ok = rig({ shots: () => jpeg(433, 2047, { size: MAX_PICTURE_BYTES }) });
  const reply = await answer(request(), ok.fake.chrome);
  assert.equal(pictureBytes(reply.result.image.data), MAX_PICTURE_BYTES);

  const over = rig({ shots: () => jpeg(433, 2047, { size: MAX_PICTURE_BYTES + 1 }) });
  const refusedReply = await answer(request(), over.fake.chrome);
  assert.deepEqual(refusedReply, failed("picture_big"));
  assert.equal(SENTENCES.picture_big(), "the picture is over 8 MiB, which is the most a picture may be, so none was returned");
  assert.equal(JSON.stringify(refusedReply).includes("/9j/"), false);
  assert.equal(shotsOf(over.fake).length, 1, "a picture over the cap is not retaken at a smaller scale");
});

test("a page that left the address while the person decided is not captured", async () => {
  const r = rig({ urls: [URL_A, "https://example.test/other"] });
  assert.deepEqual(await answer(request(), r.fake.chrome), failed("left"));
  assert.deepEqual(shotsOf(r.fake), []);
  assert.equal(detaches(r.fake), 1);
  // A page that is not at the address when it is first looked at is not asked about.
  const away = rig({ urls: ["https://example.test/other"] });
  assert.deepEqual(await answer(request(), away.fake.chrome), failed("left"));
  assert.deepEqual(away.windows, []);
});

test("a page with no size, a report that is not a size, and a page too long and narrow for a pixel each end the call with a sentence and take no picture", async () => {
  const cases = {
    "no content size": { metrics: {}, reply: failed("failed") },
    "a size that is not a number": { metrics: { cssContentSize: { width: "wide", height: 100 } }, reply: failed("failed") },
    "a size that is not finite": { metrics: { cssContentSize: { width: Infinity, height: 100 } }, reply: failed("failed") },
    "no width": { metrics: { cssContentSize: { width: 0, height: 100 } }, reply: failed("empty") },
    "a negative height": { metrics: { cssContentSize: { width: 100, height: -5 } }, reply: failed("empty") },
    "a thin page": { metrics: { cssContentSize: { width: 10, height: 30_000_000 } }, reply: failed("proportions") },
  };
  for (const [label, { metrics, reply }] of Object.entries(cases)) {
    const r = rig({ metrics });
    assert.deepEqual(await answer(request(), r.fake.chrome), reply, label);
    assert.deepEqual(r.windows, [], `${label}: the person was asked about a picture that cannot be made`);
    assert.deepEqual(shotsOf(r.fake), [], label);
    assert.equal(detaches(r.fake), 1, label);
  }
});

test("data the browser gives that is not a JPEG of base64 ends the call with the generic failure and returns nothing", async () => {
  const cases = {
    "no data": undefined,
    "a number": 5,
    "an empty string": "",
    "text": "not base64!",
    "a length that is not a multiple of four": "AAAAA",
    "a PNG": Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]).toString("base64"),
    "a JPEG without a frame header": Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0, 0, 0xff, 0xd9, 0, 0]).toString("base64"),
    "a JPEG of no width": jpeg(0, 100),
  };
  for (const [label, data] of Object.entries(cases)) {
    const r = rig({ taken: () => ({ data }) });
    assert.deepEqual(await answer(request(), r.fake.chrome), failed("failed"), label);
    assert.equal(detaches(r.fake), 1, label);
  }
});

test("a capture the browser never answers ends the call within the limit, detaches, and leaves the extension free for the next request", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  const r = rig({ taken: () => new Promise(() => {}) });
  const waiting = answer(request(), r.fake.chrome);
  await settle();
  assert.equal(shotsOf(r.fake).length, 1);
  mock.timers.tick(CAPTURE_WAIT_MS - 1);
  await settle();
  assert.equal(detaches(r.fake), 0, "the call ended early");
  mock.timers.tick(1);
  assert.deepEqual(await waiting, failed("failed"));
  assert.equal(detaches(r.fake), 1);
  // Another request is answered: the one at fault is not still holding the place.
  const next = rig();
  assert.ok((await answer(request(), next.fake.chrome)).result.image);
});

test("the debugger is detached after every path of the call, including a throw while the page is measured and one by the capture", async () => {
  const throwing = (method) => (asked) => (asked === method ? new Error(`${method} failed`) : undefined);
  for (const method of ["Page.enable", "Page.getFrameTree", "Page.getLayoutMetrics", "Page.bringToFront", SHOT]) {
    const r = rig({ failOn: { sendCommand: throwing(method) } });
    assert.deepEqual(await answer(request(), r.fake.chrome), failed("failed"), method);
    assert.equal(callsOf(r.fake, "attach").length, 1, method);
    assert.equal(detaches(r.fake), 1, method);
    assert.equal(r.fake.attached.size, 0, method);
  }
});

test("the request takes the address and nothing else, and a second tab at the address ends the call before a debugger is attached", async () => {
  const r = rig();
  for (const extra of [
    { role: "button" },
    { name: "x" },
    { x: 1 },
    { y: 1 },
    { tabId: 7 },
    { clip: { x: 0, y: 0, width: 1, height: 1, scale: 1 } },
    { scale: 1 },
    { format: "png" },
    { quality: 100 },
    { selector: "body" },
  ]) {
    const reply = await answer(request(extra), r.fake.chrome);
    assert.deepEqual(reply, refused("extra"), JSON.stringify(extra));
  }
  assert.deepEqual(await answer({ v: 1, method: METHOD, params: {} }, r.fake.chrome), refused("argument", "url"));
  assert.deepEqual(await answer(request({ url: "chrome://settings/" }), r.fake.chrome), refused("argument", "url"));
  assert.deepEqual(await answer(request({ url: "file:///etc/passwd" }), r.fake.chrome), refused("argument", "url"));
  assert.deepEqual(callsOf(r.fake, "attach"), []);

  const twice = fakeDebugger({ targets: [pageAt(URL_A, 7), pageAt(URL_A, 8)] });
  setConfirmation(async () => assert.fail("a window was opened"));
  assert.deepEqual(await answer(request(), twice.chrome), failed("tabs", 2));
  assert.deepEqual(callsOf(twice, "attach"), []);
});

test("the reply holds the picture and nothing else whatever the tool returns, and a picture that is not a JPEG of base64 within the cap is the generic failure", async () => {
  const good = jpeg(100, 100);
  const handlers = (outcome) => ({ [METHOD]: async () => outcome });
  const ask = (outcome) => answer(request(), "the browser", handlers(outcome));
  assert.deepEqual(await ask({ image: { mimeType: PICTURE_TYPE, data: good }, origin: ORIGIN, title: "Secret" }), {
    v: 1,
    result: { image: { mimeType: PICTURE_TYPE, data: good } },
  });
  for (const outcome of [
    { status: "clicked", origin: ORIGIN },
    { image: { mimeType: "image/png", data: good } },
    { image: { mimeType: PICTURE_TYPE, data: good, caption: "x" } },
    { image: { mimeType: PICTURE_TYPE } },
    { image: { mimeType: PICTURE_TYPE, data: "not base64" } },
    { image: { mimeType: PICTURE_TYPE, data: jpeg(100, 100, { size: MAX_PICTURE_BYTES + 1 }) } },
    { image: good },
    { image: [PICTURE_TYPE, good] },
    undefined,
    null,
    "text",
  ]) {
    assert.deepEqual(await ask(outcome), failed("failed"), JSON.stringify(outcome)?.slice(0, 80));
  }
  // A method that is not a picture method is never given a picture's shape.
  const other = await answer(
    { v: 1, method: "hover_element", params: { url: URL_A, role: "link", name: "Docs" } },
    "the browser",
    { hover_element: async () => ({ image: { mimeType: PICTURE_TYPE, data: good } }) },
  );
  assert.deepEqual(other, failed("failed"));
  assert.deepEqual(PICTURES, [METHOD]);
});

test("an image is a picture only when its base64 is canonical: the alphabet, a length of a multiple of four, padding only at the end", () => {
  assert.equal(pictureBytes("QQ=="), 1);
  assert.equal(pictureBytes("QUI="), 2);
  assert.equal(pictureBytes("QUJD"), 3);
  assert.equal(pictureBytes("QUJDRA=="), 4);
  for (const bad of [undefined, null, 5, {}, [], "", "====", "QQ=", "Q===", "QQ==QQ==", "QU-_", "QU JD", "QUJD\n", "QUJ", "=QUJ"]) {
    assert.equal(pictureBytes(bad), null, JSON.stringify(bad));
  }
  assert.equal(isPicture({ mimeType: PICTURE_TYPE, data: "QUJD" }), true);
  assert.equal(isPicture({ data: "QUJD", mimeType: PICTURE_TYPE }), true);
  for (const bad of [
    null,
    "QUJD",
    [],
    {},
    { mimeType: PICTURE_TYPE },
    { data: "QUJD" },
    { mimeType: "image/png", data: "QUJD" },
    { mimeType: "image/svg+xml", data: "QUJD" },
    { mimeType: PICTURE_TYPE, data: "QUJD", extra: 1 },
    { mimeType: PICTURE_TYPE, data: "" },
  ]) {
    assert.equal(isPicture(bad), false, JSON.stringify(bad));
  }
  assert.equal(MAX_PICTURE_BYTES, 8 * 1024 * 1024);
});

test("the size of a JPEG is read from its frame header wherever it is: after a colour profile, in a baseline or a progressive frame, and in the start of pictures a real browser made", () => {
  assert.deepEqual(jpegSize(jpeg(433, 2047)), { width: 433, height: 2047 });
  assert.deepEqual(jpegSize(jpeg(2048, 245, { progressive: true })), { width: 2048, height: 245 });
  // A profile of 580 bytes puts the frame header at byte 632, as Brave's pictures have it.
  assert.deepEqual(jpegSize(jpeg(1280, 719, { profile: 580 })), { width: 1280, height: 719 });
  assert.deepEqual(jpegSize(jpeg(70, 65535, { profile: 40_000 })), { width: 70, height: 65535 });
  // A header that does not end in the first 49152 bytes is not read.
  assert.equal(jpegSize(jpeg(70, 100, { profile: 60_000 })), null);
  // The start of the pictures a real Brave made.
  assert.deepEqual(jpegSize(BRAVE_TALL), { width: 434, height: 2048 });
  assert.deepEqual(jpegSize(BRAVE_WIDE), { width: 2048, height: 226 });
  // Sizes of 256 and above use both bytes.
  assert.deepEqual(jpegSize(jpeg(256, 257)), { width: 256, height: 257 });
  // A header that is not one.
  assert.equal(jpegSize(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0, 0, 0, 0]).toString("base64")), null);
  assert.equal(jpegSize(Buffer.from([0xff, 0xd8, 0xff, 0xd9, 0, 0, 0, 0]).toString("base64")), null);
  assert.equal(jpegSize(Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0, 0, 0x00, 0x00, 0, 0]).toString("base64")), null);
  // A frame header cut short, and no data at all.
  const cut = [0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0, 0, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00];
  assert.equal(jpegSize(Buffer.from(cut).toString("base64")), null);
  assert.equal(jpegSize(""), null);
});

test("this extension names Page.captureScreenshot in the list of cdp.js and sends it from capture.js alone, and sends it only with a token", async () => {
  const CODE = scripts();
  assert.deepEqual(occurrences(CODE, "Page\\.captureScreenshot"), { "capture.js": 1, "cdp.js": 1 });
  assert.equal(ALLOWED[SHOT], true);
  assert.equal(Object.keys(ALLOWED).some((method) => method.startsWith("Emulation.")), false);
  assert.equal(Object.keys(ALLOWED).some((method) => method.startsWith("Runtime.")), false);
  // With no approval given, every token is refused and nothing is sent.
  const r = rig();
  for (const token of [undefined, null, {}, "yes", approve({})]) {
    await withSession(r.fake.chrome, URL_A, async (session) => {
      await command(session, "Page.enable");
      await assert.rejects(
        command(session, SHOT, { format: "jpeg" }, token),
        (error) => error instanceof Refusal && error.code === "not_allowed",
      );
    });
  }
  assert.deepEqual(shotsOf(r.fake), []);
});

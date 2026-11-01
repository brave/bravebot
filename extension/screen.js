// The screen group: a picture of what the person chooses to share from the
// browser's own picker, and an animated GIF of an open page.
//
// capture_screen opens a small window of the extension (capture.html). The
// window asks the browser for its picker with desktopCapture, takes one frame of
// what the person chose with getUserMedia, stops the stream, and sends the worker
// the frame as a JPEG. The picker, the stream and the frame are all in that one
// page: the browser lets a stream id be used only by the frame that asked for it,
// and the extension's other pages hold no desktopCapture. Nothing is captured
// until the person chooses in the picker, which the browser shows for each call.
//
// record_gif takes frames of a tab with the tool that takes a picture of a page,
// decodes each, and writes them as a GIF (gif.js).
//
// This file imports nothing of tools.js, which imports it.

import { Refusal, ToolError } from "./errors.js";
import { encodeGif } from "./gif.js";

// The page of the window the person chooses in, and the size of the window.
export const CAPTURE_PAGE = "capture.html";
const WINDOW_SIZE = Object.freeze({ width: 480, height: 320 });

// The kind of the message the window sends the worker with its outcome.
export const CAPTURE_MESSAGE = "bravebot-capture";

// How long the person has to choose in the picker before the call fails, in
// milliseconds. The relay's wait for a reply from the extension is 30 seconds
// longer (IN_BROWSER_REPLY_TIMEOUT in crates/browser/src/relay.rs).
export const CAPTURE_WAIT_MS = 60_000;

// What the picker offers: screens and windows of any program, and not tabs of
// the browser, which screenshot_page takes.
export const PICKER_SOURCES = Object.freeze(["screen", "window"]);

// The quality of the JPEG a frame of the screen is encoded as, and the most
// pixels of its longer side. A frame larger than that is scaled down to it.
export const SCREEN_QUALITY = 0.8;
export const SCREEN_MAX_SIDE = 2048;

// The kinds of outcome the window reports.
export const PICTURE = "picture";
export const CANCELLED = "cancelled";
export const FAILED_TO_CAPTURE = "failed";

// Why the window failed, as the window names it. The worker says the sentence,
// so nothing the window sends is shown but a name from this list.
export const WINDOW_FAILURES = Object.freeze({
  picker: "the browser would not show its picker",
  stream: "the browser would not open what was chosen",
  frame: "what was chosen gave no picture",
  encoding: "the picture could not be encoded",
});

// The size a picture of `width` by `height` is scaled to so that its longer
// side is at most `side`, keeping its proportions and at least one pixel on a
// side.
export function fitWithin(width, height, side) {
  const longer = Math.max(width, height);
  if (longer <= side) {
    return { width, height };
  }
  const scale = side / longer;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

// Why a capture did not produce a picture. Each is a sentence of ours, with
// nothing from the screen in it.
const NOT_TAKEN = Object.freeze({
  cancelled:
    "you closed the browser's picker without choosing anything, so nothing was captured",
  closed: "the capture window was closed, so nothing was captured",
  timeout: `nothing was chosen in the browser within ${CAPTURE_WAIT_MS / 1000} seconds, so nothing was captured`,
});

// The capture waiting for the window of the browser `chrome`, if any, kept per
// browser so a test needs no reset.
const captures = new WeakMap();

// Whether `sender` is the page of the window opened for `waiting`: from this
// extension, from the page of the window, in its top frame, and from the window
// that was opened.
async function isTheWindow(chrome, waiting, sender) {
  let opened;
  try {
    opened = await waiting.opened;
  } catch {
    return false;
  }
  return (
    sender?.id === chrome.runtime.id &&
    typeof sender.url === "string" &&
    sender.url.split(/[?#]/)[0] === chrome.runtime.getURL(CAPTURE_PAGE) &&
    (sender.frameId ?? 0) === 0 &&
    sender.tab?.windowId === opened?.id
  );
}

// Registers the listeners the capture window needs. They are registered as the
// worker starts, since the browser reports an event only to a listener that was
// there. The window's message is the only way to an outcome other than closing
// it or letting the time run out, and it is taken from the window that was
// opened for the waiting call alone.
export function listenForCapture(chrome) {
  chrome.runtime.onMessage.addListener((message, sender, respond) => {
    const waiting = captures.get(chrome);
    if (message?.type !== CAPTURE_MESSAGE || !waiting) {
      return false;
    }
    isTheWindow(chrome, waiting, sender).then((yes) => {
      if (yes && captures.get(chrome) === waiting) {
        waiting.settle({ ended: "reported", message });
      }
      respond({});
    });
    return true;
  });
  chrome.windows.onRemoved.addListener((windowId) => {
    const waiting = captures.get(chrome);
    waiting?.opened
      ?.then((opened) => {
        if (opened.id === windowId) {
          waiting.settle({ ended: "closed" });
        }
      })
      .catch(() => {});
  });
}

// Closes the window opened for `waiting`, if it is open.
async function closeWindow(chrome, waiting) {
  try {
    const opened = await waiting.opened;
    await chrome.windows.remove(opened.id);
  } catch {
    // Not opened, or already closed.
  }
}

// Opens the capture window, waits for the person to choose in the picker, and
// answers the frame as a JPEG data URL. It fails, saying why, when the person
// closes the picker or the window, when nothing is chosen in CAPTURE_WAIT_MS,
// or when the browser cannot show the picker or open what was chosen. The
// window is closed on every path.
export async function captureScreen(chrome) {
  if (captures.get(chrome)) {
    throw new Refusal(
      "another capture is waiting for your choice in the browser, so nothing was captured",
    );
  }
  const waiting = { opened: null, settle: null };
  captures.set(chrome, waiting);
  const outcome = new Promise((resolve) => {
    waiting.settle = resolve;
  });
  const timer = setTimeout(waiting.settle, CAPTURE_WAIT_MS, {
    ended: "timeout",
  });
  try {
    waiting.opened = chrome.windows.create({
      url: chrome.runtime.getURL(CAPTURE_PAGE),
      type: "popup",
      focused: true,
      ...WINDOW_SIZE,
    });
    waiting.opened.catch((error) => waiting.settle({ ended: "unopened", error }));
    const { ended, error, message } = await outcome;
    if (ended === "unopened") {
      throw new ToolError(
        `the capture window could not be opened: ${error?.message ?? error}`,
      );
    }
    if (ended === "timeout" || ended === "closed") {
      throw new Refusal(NOT_TAKEN[ended]);
    }
    if (message.outcome === CANCELLED) {
      throw new Refusal(NOT_TAKEN.cancelled);
    }
    if (message.outcome === PICTURE && typeof message.data === "string") {
      return message.data;
    }
    const why = Object.hasOwn(WINDOW_FAILURES, message.reason)
      ? WINDOW_FAILURES[message.reason]
      : "the window gave no picture";
    throw new ToolError(`the screen could not be captured: ${why}`);
  } finally {
    clearTimeout(timer);
    captures.delete(chrome);
    await closeWindow(chrome, waiting);
  }
}

// The size a frame of a recording is scaled to, on its longer side, in pixels.
// A recording is at most MAX_GIF_FRAMES of MAX_GIF_SIDE by MAX_GIF_SIDE, which
// gif.js writes in under 6.9 MB, below the 8 MiB the client takes.
export const MAX_GIF_SIDE = 480;
export const DEFAULT_GIF_FRAMES = 10;
export const MAX_GIF_FRAMES = 20;

// The least and most time a frame stays, in hundredths of a second. A frame
// stays as long as the next took to arrive, so the GIF plays at the speed the
// page moved at.
const LEAST_DELAY = 2;
const MOST_DELAY = 300;

// The pixels of a JPEG given as base64, scaled to fit MAX_GIF_SIDE, or to
// `size` where an earlier frame fixed it. The scaling and the reading of the
// pixels are the browser's (createImageBitmap and OffscreenCanvas, both in a
// service worker).
async function pixelsOf(base64, size) {
  const bytes = Uint8Array.from(atob(base64), (character) =>
    character.charCodeAt(0),
  );
  const bitmap = await createImageBitmap(
    new Blob([bytes], { type: "image/jpeg" }),
  );
  try {
    const { width, height } =
      size ?? fitWithin(bitmap.width, bitmap.height, MAX_GIF_SIDE);
    const context = new OffscreenCanvas(width, height).getContext("2d", {
      willReadFrequently: true,
    });
    context.drawImage(bitmap, 0, 0, width, height);
    return { width, height, rgba: context.getImageData(0, 0, width, height).data };
  } finally {
    bitmap.close();
  }
}

// `bytes` as base64.
export function toBase64(bytes) {
  let text = "";
  for (let at = 0; at < bytes.length; at += 0x8000) {
    text += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  }
  return btoa(text);
}

// Takes `count` frames with `takeFrame`, which answers the base64 of a JPEG,
// and answers them as the bytes of an animated GIF. One frame that cannot be
// taken fails the recording, saying which, so a recording never holds a frame
// of a page the call did not name. The frames are scaled to the size of the
// first.
export async function recordGif(count, takeFrame) {
  const frames = [];
  let size;
  for (let number = 1; number <= count; number += 1) {
    let base64;
    try {
      base64 = await takeFrame();
    } catch (error) {
      throw new ToolError(
        `frame ${number} of ${count} could not be taken, so there is no recording: ${error?.message ?? error}`,
      );
    }
    const at = Date.now();
    const frame = await pixelsOf(base64, size);
    size ??= { width: frame.width, height: frame.height };
    frames.push({ rgba: frame.rgba, at });
  }
  const gaps = frames.map((frame, number) =>
    number + 1 < frames.length ? frames[number + 1].at - frame.at : null,
  );
  gaps[gaps.length - 1] = gaps[gaps.length - 2];
  return encodeGif({
    ...size,
    frames: frames.map((frame) => frame.rgba),
    delays: gaps.map((gap) =>
      Math.min(MOST_DELAY, Math.max(LEAST_DELAY, Math.round(gap / 10))),
    ),
  });
}

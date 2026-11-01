// The capture window: the page in the small window the service worker opens for
// capture_screen. It asks the browser for its picker, takes one frame of what the
// person chose, and sends the worker the frame as a JPEG. The picker, the stream
// and the frame are all here because the browser lets only the frame that asked
// for a stream use it (screen.js).
//
// The window is an extension page: no web page can open it or send it a message.
// The worker accepts what it sends from this window alone, and only while it
// waits for it. The stream is stopped on every path, before the outcome is sent.

import {
  CANCELLED,
  CAPTURE_MESSAGE,
  FAILED_TO_CAPTURE,
  PICKER_SOURCES,
  PICTURE,
  SCREEN_MAX_SIDE,
  SCREEN_QUALITY,
  fitWithin,
} from "./screen.js";

const status = document.getElementById("status");
const cancel = document.getElementById("cancel");

// How long to wait for the first frame of the stream, and then for a second one,
// in milliseconds. A window whose content does not change delivers one frame and
// no more.
const FRAME_WAIT_MS = 5_000;
const NEXT_FRAME_WAIT_MS = 1_000;

// What went wrong, as a name from WINDOW_FAILURES in screen.js.
class Failure extends Error {}

// The browser's request for its picker, kept so the picker can be hidden when
// the person cancels here or this window goes away.
let request = null;

function hidePicker() {
  if (request !== null) {
    try {
      chrome.desktopCapture.cancelChooseDesktopMedia(request);
    } catch {
      // The picker is already gone.
    }
  }
}

// The id of what the person chose in the browser's picker, or "" when they
// closed it.
function choose() {
  return new Promise((resolve, reject) => {
    let answered = false;
    try {
      const asked = chrome.desktopCapture.chooseDesktopMedia(
        PICKER_SOURCES,
        (id) => {
          answered = true;
          request = null;
          if (chrome.runtime.lastError) {
            reject(new Failure("picker"));
          } else {
            resolve(id ?? "");
          }
        },
      );
      request = answered ? null : asked;
    } catch {
      reject(new Failure("picker"));
    }
  });
}

// Waits for the first frame of `video` and then for a second one, since the
// first can be blank, and goes on with the first when no second comes in time.
function frames(video) {
  return new Promise((resolve, reject) => {
    let timer = setTimeout(() => reject(new Failure("frame")), FRAME_WAIT_MS);
    video.requestVideoFrameCallback(() => {
      clearTimeout(timer);
      timer = setTimeout(resolve, NEXT_FRAME_WAIT_MS);
      video.requestVideoFrameCallback(() => {
        clearTimeout(timer);
        resolve();
      });
    });
  });
}

// One frame of what the stream shows, as a JPEG data URL.
async function frameOf(stream) {
  const video = document.createElement("video");
  video.muted = true;
  video.srcObject = stream;
  try {
    await video.play();
    await frames(video);
    if (!(video.videoWidth > 0 && video.videoHeight > 0)) {
      throw new Failure("frame");
    }
    const { width, height } = fitWithin(
      video.videoWidth,
      video.videoHeight,
      SCREEN_MAX_SIDE,
    );
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    canvas.getContext("2d").drawImage(video, 0, 0, width, height);
    const data = canvas.toDataURL("image/jpeg", SCREEN_QUALITY);
    if (!data.startsWith("data:image/jpeg;base64,")) {
      throw new Failure("encoding");
    }
    return data;
  } catch (error) {
    throw error instanceof Failure ? error : new Failure("frame");
  } finally {
    video.srcObject = null;
  }
}

// What the person chose, as one frame, or the outcome that says they chose
// nothing. The stream is stopped before the frame is returned.
async function capture() {
  const id = await choose();
  if (id === "") {
    return { outcome: CANCELLED };
  }
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        mandatory: { chromeMediaSource: "desktop", chromeMediaSourceId: id },
      },
    });
  } catch {
    throw new Failure("stream");
  }
  try {
    return { outcome: PICTURE, data: await frameOf(stream) };
  } finally {
    stream.getTracks().forEach((track) => track.stop());
  }
}

async function run() {
  let message;
  try {
    status.textContent = "Waiting for your choice in the browser.";
    message = await capture();
    status.textContent =
      message.outcome === PICTURE
        ? "Done. This window closes."
        : "Nothing was chosen. This window closes.";
  } catch (error) {
    message = {
      outcome: FAILED_TO_CAPTURE,
      reason: error instanceof Failure ? error.message : "stream",
    };
    status.textContent = "The screen could not be captured. This window closes.";
  }
  try {
    await chrome.runtime.sendMessage({ type: CAPTURE_MESSAGE, ...message });
  } catch {
    // The worker is gone; the window ends itself.
  }
  window.close();
}

cancel.addEventListener("click", hidePicker);
window.addEventListener("pagehide", hidePicker);
run();

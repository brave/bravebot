// The tool screenshot_full_page: a picture of the whole top document of the one
// tab at an address, from the top of the page to the bottom and the left to the
// right, including the parts out of view and the parts nobody can see. The
// browser draws it as it draws the page, so a frame is in the picture as the page
// shows it, in the box the page gives it, and the content of a frame beyond that
// box is not. Hidden text and parts covered by another element are in it only
// as far as the browser draws them.
//
// The person is asked in the confirmation window, which shows the page's origin
// and address and the size of its content and lists no element, and nothing is
// captured before they allow it. The picture is a JPEG at quality 80, scaled so
// that its long side is at most 2048 pixels, and it is returned only if it is at
// most 8 MiB once decoded. A picture over either is never cut or resized by this
// extension: it is taken again at a smaller scale once, and then refused.
//
// This extension reads the picture's header for its size and nothing else of it.
// The reply holds the picture and no other field (requests.js).

import { describe } from "./actions.js";
import { command, withSession } from "./cdp.js";
import { Failure } from "./sentences.js";

// The picture is a JPEG, which is what the browser is asked for. A JPEG of this
// quality is a fifth of the PNG of the same page.
export const PICTURE_TYPE = "image/jpeg";
export const PICTURE_QUALITY = 80;

// The most pixels on the long side of a picture.
export const LONG_SIDE = 2048;

// The largest picture returned, in bytes once decoded. It is the most the
// BraveBot client takes from a tool (MAX_PICTURE_BYTES in crates/mcp/src/protocol.rs
// and in extension/pictures.js).
export const MAX_PICTURE_BYTES = 8 * 1024 * 1024;

// How long the capture may take once the person has allowed it, in milliseconds,
// for the first picture and the second together. It is inside the time a call has
// after the person's answer (cdp.js CALL_WAIT_MS), which ends the call whatever it
// is doing, so the capture's own end comes first and says the generic failure.
export const CAPTURE_WAIT_MS = 10_000;

// How many characters of the picture's base64 are read to find the size in the
// JPEG header. Four to a group of three bytes, and a multiple of four, so a
// prefix is itself base64. The header of a picture the browser makes ends in the
// first thousand bytes.
const HEADER_CHARS = 65_536;

// The JPEG markers that start a frame and hold its size: baseline, extended
// sequential and progressive.
const FRAMES = Object.freeze([0xc0, 0xc1, 0xc2]);

// How many times the picture is taken: once at the scale the page's size gives,
// and once more at a scale worked out from the size the browser gave.
const ATTEMPTS = 2;

// The scale of a clip is kept to this many decimals, rounded down, so that the
// picture is never a pixel over for a rounding.
const SCALE_STEP = 10_000;

// The number of bytes `data` decodes to, or null if it is not a non-empty string
// in the canonical form of base64: the standard alphabet, a multiple of four
// characters and padding only at the end.
export function pictureBytes(data) {
  if (
    typeof data !== "string" ||
    data.length === 0 ||
    data.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]+={0,2}$/.test(data)
  ) {
    return null;
  }
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
  return (data.length / 4) * 3 - padding;
}

// Whether `image` is the picture a reply may hold: the object {mimeType, data}
// and nothing else, a JPEG, as canonical base64 that decodes to at most
// MAX_PICTURE_BYTES.
export function isPicture(image) {
  if (
    typeof image !== "object" ||
    image === null ||
    Array.isArray(image) ||
    Object.keys(image).sort().join() !== "data,mimeType" ||
    image.mimeType !== PICTURE_TYPE
  ) {
    return false;
  }
  const bytes = pictureBytes(image.data);
  return bytes !== null && bytes <= MAX_PICTURE_BYTES;
}

// The width and height of the JPEG that `data` is, from the header, or null. It
// walks the segments from the start of the file to the first frame header.
export function jpegSize(data) {
  const head = atob(data.slice(0, HEADER_CHARS));
  const byte = (at) => head.charCodeAt(at);
  if (byte(0) !== 0xff || byte(1) !== 0xd8) {
    return null;
  }
  let at = 2;
  while (at + 3 < head.length) {
    if (byte(at) !== 0xff) {
      return null;
    }
    const marker = byte(at + 1);
    if (marker === 0xff) {
      at += 1;
    } else if (FRAMES.includes(marker)) {
      if (at + 8 >= head.length) {
        return null;
      }
      return {
        height: byte(at + 5) * 256 + byte(at + 6),
        width: byte(at + 7) * 256 + byte(at + 8),
      };
    } else if (marker === 0xd9 || marker === 0xda) {
      return null;
    } else {
      at += 2 + byte(at + 2) * 256 + byte(at + 3);
    }
  }
  return null;
}

// Fails unless the tab is at `url`, and answers the size of the page's content in
// CSS pixels, and how many pixels the browser makes of one CSS pixel. The
// browser reports the content in device pixels as well, so a screen that draws
// two pixels for one gives a factor of 2. A report that is not a size is the
// generic failure.
async function measure(session, url) {
  const { frame } = (await command(session, "Page.getFrameTree")).frameTree;
  if (`${frame.url}${frame.urlFragment ?? ""}` !== url) {
    throw new Failure("left");
  }
  const metrics = await command(session, "Page.getLayoutMetrics");
  const { width, height } = metrics?.cssContentSize ?? {};
  if (![width, height].every((side) => Number.isFinite(side))) {
    throw new Failure("failed");
  }
  if (width <= 0 || height <= 0) {
    throw new Failure("empty");
  }
  const device = metrics.contentSize?.width;
  const factor = Number.isFinite(device) && device > 0 ? device / width : 1;
  return Object.freeze({ width: Math.ceil(width), height: Math.ceil(height), factor });
}

// The page's size, for the window.
const sizeOf = ({ width, height }) => `${width} by ${height} CSS pixels`;

// A scale rounded down to SCALE_STEP, so that the picture is never a pixel over
// for a rounding.
const floorScale = (scale) => Math.floor(scale * SCALE_STEP) / SCALE_STEP;

// The scale at which a page of `page`'s size is a picture whose long side is at
// most LONG_SIDE pixels, or a failure if its short side would be under a pixel.
function firstScale(page) {
  const long = Math.max(page.width, page.height) * page.factor;
  const scale = floorScale(Math.min(1, LONG_SIDE / long));
  if (Math.min(page.width, page.height) * page.factor * scale < 1) {
    throw new Failure("proportions");
  }
  return scale;
}

// The picture taken at `scale` with the person's token, as base64 of a JPEG and
// the size the browser made it, or a failure if the browser gave something that
// is not one.
async function capture(session, page, scale, token) {
  const taken = await command(
    session,
    "Page.captureScreenshot",
    {
      format: "jpeg",
      quality: PICTURE_QUALITY,
      captureBeyondViewport: true,
      fromSurface: true,
      clip: { x: 0, y: 0, width: page.width, height: page.height, scale },
    },
    token,
  );
  const data = taken?.data;
  const bytes = pictureBytes(data);
  const size = bytes === null ? null : jpegSize(data);
  if (size === null || size.width < 1 || size.height < 1) {
    throw new Failure("failed");
  }
  return { data, bytes, size };
}

// Takes the picture with the person's token, and answers its base64. The page's
// size is read again, since the page may have changed while the person decided.
// A picture over LONG_SIDE is taken again at the scale its size gives. The first
// scale allows for the pixels the browser makes of a CSS pixel, so that is the
// case only where the browser reports that wrongly.
async function takeWith(session, url, token) {
  const page = await measure(session, url);
  let scale = firstScale(page);
  for (let attempt = 1; ; attempt += 1) {
    const { data, bytes, size } = await capture(session, page, scale, token);
    const long = Math.max(size.width, size.height);
    if (long <= LONG_SIDE) {
      if (bytes > MAX_PICTURE_BYTES) {
        throw new Failure("picture_big");
      }
      return data;
    }
    if (attempt === ATTEMPTS) {
      throw new Failure("picture_size");
    }
    const smaller = floorScale((scale * LONG_SIDE) / long);
    if ((Math.min(size.width, size.height) * smaller) / scale < 1) {
      throw new Failure("proportions");
    }
    scale = smaller;
  }
}

// takeWith, ended with a failure if it takes longer than CAPTURE_WAIT_MS.
async function shoot(session, url, token) {
  let timer;
  const stuck = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Failure("failed")), CAPTURE_WAIT_MS);
  });
  try {
    return await Promise.race([takeWith(session, url, token), stuck]);
  } finally {
    clearTimeout(timer);
  }
}

// The picture of the page at `args.url`, as {image: {mimeType, data}}, taken
// after the person has allowed it in the window. `permit` is the call's approval
// (tools.js permission()), which accepts the token the window makes for the
// resolution it is asked about and no other.
export async function takePicture(chrome, { url }, permit) {
  const origin = new URL(url).origin;
  const data = await withSession(
    chrome,
    url,
    async (session) => {
      await command(session, "Page.enable");
      const page = await measure(session, url);
      // A page no picture can be made of is refused before the person is asked.
      firstScale(page);
      await command(session, "Page.bringToFront");
      const resolution = Object.freeze({
        description: describe("picture", { origin, url, size: sizeOf(page) }),
      });
      const token = await permit.ask(resolution);
      return shoot(session, url, token);
    },
    { allows: permit.allows, time: permit.time },
  );
  return Object.freeze({ image: Object.freeze({ mimeType: PICTURE_TYPE, data }) });
}

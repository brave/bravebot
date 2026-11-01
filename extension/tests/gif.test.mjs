// The GIF encoder behind record_gif. The bytes it writes are read back with a
// reader written here from the format's description, which shares no code with
// the encoder, and a small known GIF is compared byte for byte.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import { MAX_COLORS, encodeGif, lzw } from "../gif.js";
import { readGif, unpack } from "./gif-reader.mjs";

// A channel at the 5-bit precision the encoder counts colours to, widened to 8
// bits with its top bits repeated.
const widen = (value) => ((value >> 3) << 3) | (value >> 5);

// A deterministic stream of numbers.
function random(seed) {
  let state = seed;
  return () => (state = (state * 1664525 + 1013904223) >>> 0) / 2 ** 32;
}

// Pixels of `colors` distinct colours, each channel a multiple of 8 so the
// encoder's 5-bit precision loses nothing but the widening.
function picture(width, height, colors, seed) {
  const next = random(seed);
  const palette = new Map();
  while (palette.size < colors) {
    const color = [0, 0, 0].map(() => Math.floor(next() * 32) * 8 + 3);
    palette.set(color.join(), color);
  }
  const list = [...palette.values()];
  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const [r, g, b] = list[Math.floor(next() * list.length)];
    rgba.set([r, g, b, 255], pixel * 4);
  }
  return rgba;
}

const colorAt = (gif, frame, pixel) =>
  [0, 1, 2].map((k) => gif.palette[gif.frames[frame].indices[pixel] * 3 + k]);

// A 2 by 2 picture of four colours in two frames, as the bytes the encoder is
// pinned to. A GIF reader (macOS sips, Brave's createImageBitmap) shows the
// first frame as red, green, blue and white.
test("a small recording is written as the bytes of a GIF89a with a looping extension, a delay for each frame and one 256 colour palette", () => {
  const first = Uint8ClampedArray.from([
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
  ]);
  const second = Uint8ClampedArray.from([
    0, 0, 0, 255, 255, 255, 0, 255, 0, 255, 255, 255, 128, 128, 128, 255,
  ]);
  const bytes = encodeGif({
    width: 2,
    height: 2,
    frames: [first, second],
    delays: [60, 70],
  });
  const gif = readGif(bytes);
  assert.deepEqual([gif.width, gif.height, gif.frames.length], [2, 2, 2]);
  assert.deepEqual(
    gif.frames.map((frame) => frame.delay),
    [60, 70],
  );
  assert.equal(gif.loops, 0, "repeats for ever");
  assert.equal(gif.palette.length, 768);
  const shown = (frame) =>
    [0, 1, 2, 3].map((pixel) => colorAt(gif, frame, pixel).map((v) => v >> 3));
  assert.deepEqual(shown(0), [
    [31, 0, 0],
    [0, 31, 0],
    [0, 0, 31],
    [31, 31, 31],
  ]);
  assert.deepEqual(shown(1), [
    [0, 0, 0],
    [31, 31, 0],
    [0, 31, 31],
    [16, 16, 16],
  ]);
  // The exact bytes, which macOS sips and Brave's createImageBitmap read as the
  // two frames above: the screen, the eight colours in order of their 5-bit
  // key (black, blue, green, cyan, red, grey, yellow, white) and black after
  // them, the looping extension, and for each frame its delay, its descriptor,
  // the code size and the LZW data in one block.
  const hex = Buffer.from(bytes).toString("hex");
  assert.equal(hex.slice(0, 26), "47494638396102000200f70000");
  assert.equal(
    hex.slice(26, 26 + 48),
    "000000" + "0000ff" + "00ff00" + "00ffff" + "ff0000" + "848484" + "ffff00" + "ffffff",
  );
  assert.match(hex.slice(26 + 48, 26 + 1536), /^0+$/);
  assert.equal(
    hex.slice(26 + 1536),
    "21ff0b4e45545343415045322e3003010000" + "00" +
      "21f904043c000000" + "2c0000000002000200" + "00" + "08" + "07" + "00090808702020" + "00" +
      "21f90404460000" + "00" + "2c0000000002000200" + "00" + "08" + "07" + "00011818502020" + "00" +
      "3b",
  );
});

// The LZW data is the part of the format with a table that grows. The code is
// 9 bits until the table has 512 entries and then 10, 11 and 12, and a full
// table is cleared. The reader decodes every length from 1 to 2400 pixels of
// random indices, which puts the end of the data on each side of each change of
// size in turn, and longer runs of random and of repeated indices, which fill
// the table.
test("the data of a frame decodes to the same indices for every length across each change of code size and a full table", () => {
  const next = random(7);
  for (let length = 1; length <= 2400; length += 1) {
    const indices = Array.from({ length }, () => Math.floor(next() * 256));
    assert.deepEqual(unpack(lzw(indices), 8, length), indices, `${length} indices`);
  }
  for (const length of [3_500, 3_999, 4_000, 4_100, 4_500, 6_000, 12_000, 30_000]) {
    const indices = Array.from({ length }, () => Math.floor(next() * 256));
    assert.deepEqual(unpack(lzw(indices), 8, length), indices, `${length} indices`);
  }
  for (const length of [1, 2, 600, 5_000, 70_000]) {
    const same = new Array(length).fill(3);
    assert.deepEqual(unpack(lzw(same), 8, length), same, `${length} equal indices`);
    const two = Array.from({ length }, (_, at) => (at % 7 < 3 ? 0 : 255));
    assert.deepEqual(unpack(lzw(two), 8, length), two, `${length} runs`);
  }
});

test("every pixel of a picture of up to 256 colours is written as its own colour, at 5 bits of precision, in each frame", () => {
  for (const [width, height, colors, seed] of [
    [1, 1, 1, 1],
    [3, 2, 4, 2],
    [17, 13, 50, 3],
    [64, 64, 200, 4],
    [100, 80, 256, 5],
  ]) {
    // The second frame holds the same colours in another order, so the two
    // frames together are no more than the palette holds.
    const first = picture(width, height, colors, seed);
    const second = Uint8ClampedArray.from([...first.slice(4 * 3), ...first.slice(0, 4 * 3)]);
    const frames = [first, second];
    const gif = readGif(encodeGif({ width, height, frames, delays: [5, 5] }));
    frames.forEach((rgba, frame) => {
      for (let pixel = 0; pixel < width * height; pixel += 1) {
        const want = [0, 1, 2].map((k) => widen(rgba[pixel * 4 + k]));
        assert.deepEqual(colorAt(gif, frame, pixel), want, `${width}x${height} frame ${frame} pixel ${pixel}`);
      }
    });
  }
});

// More colours than the palette holds are cut into 256 boxes, so a pixel is
// shown as the mean of its box and not as the first colour met or the last.
test("a picture of thousands of colours uses at most 256, and each pixel is shown as a colour near its own", () => {
  const width = 120;
  const height = 100;
  const next = random(11);
  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    rgba.set([next() * 256, next() * 256, next() * 256, 255], pixel * 4);
  }
  const gif = readGif(encodeGif({ width, height, frames: [rgba], delays: [5] }));
  const used = new Set(gif.frames[0].indices);
  assert.ok(used.size > 200 && used.size <= MAX_COLORS, `${used.size} colours used`);
  let error = 0;
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const shown = colorAt(gif, 0, pixel);
    for (let k = 0; k < 3; k += 1) {
      error += Math.abs(shown[k] - rgba[pixel * 4 + k]);
    }
  }
  const mean = error / (width * height * 3);
  assert.ok(mean < 20, `the mean error of a channel is ${mean.toFixed(1)}`);
});

// A palette has 256 entries, so the 257th box a cut would make is a colour the
// palette cannot hold: its pixels would be written with the first entry's colour.
test("no pixel of a picture of thousands of colours is shown as a colour far from its own, so the cut stops at 256 boxes", () => {
  assert.equal(MAX_COLORS, 256);
  const width = 120;
  const height = 100;
  const next = random(11);
  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    rgba.set([next() * 256, next() * 256, next() * 256, 255], pixel * 4);
  }
  const gif = readGif(encodeGif({ width, height, frames: [rgba], delays: [5] }));
  assert.equal(new Set(gif.frames[0].indices).size, 256);
  let worst = 0;
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const shown = colorAt(gif, 0, pixel);
    for (let k = 0; k < 3; k += 1) {
      worst = Math.max(worst, Math.abs(shown[k] - rgba[pixel * 4 + k]));
    }
  }
  // A box of 256 holds colours within about 50 of each other in a channel. A
  // pixel written with the first entry's colour is off by over 200.
  assert.ok(worst < 100, `a channel is shown ${worst} from its own`);
});

// The box with the most pixels is cut first, so a picture that is mostly one
// colour with a few others keeps each of them.
test("a colour that few pixels have keeps its own palette entry beside a colour most pixels have", () => {
  const rgba = new Uint8ClampedArray(40 * 4);
  for (let pixel = 0; pixel < 40; pixel += 1) {
    rgba.set(pixel < 36 ? [10, 10, 10, 255] : [250, 20, 20, 255], pixel * 4);
  }
  const gif = readGif(encodeGif({ width: 40, height: 1, frames: [rgba], delays: [5] }));
  assert.deepEqual(colorAt(gif, 0, 0).map((v) => v >> 3), [1, 1, 1]);
  assert.deepEqual(colorAt(gif, 0, 39).map((v) => v >> 3), [31, 2, 2]);
});

test("a delay is written in hundredths of a second, rounded, and kept within what the format holds", () => {
  const rgba = Uint8ClampedArray.from([1, 2, 3, 255]);
  const bytes = encodeGif({
    width: 1,
    height: 1,
    frames: [rgba, rgba, rgba],
    delays: [0, 61.6, 90_000],
  });
  assert.deepEqual(
    readGif(bytes).frames.map((frame) => frame.delay),
    [0, 62, 65_535],
  );
});

test("a picture or frames the format cannot hold are refused", () => {
  const rgba = new Uint8ClampedArray(4);
  const base = { width: 1, height: 1, frames: [rgba], delays: [1] };
  for (const bad of [
    { width: 0 },
    { height: 0 },
    { width: 65_536 },
    { height: 1.5 },
    { frames: [] , delays: [] },
    { delays: [] },
    { frames: [new Uint8ClampedArray(8)] },
  ]) {
    assert.throws(() => encodeGif({ ...base, ...bad }), RangeError, JSON.stringify(Object.keys(bad)));
  }
});

// A recording is at most MAX_GIF_FRAMES frames of MAX_GIF_SIDE pixels on a
// side, so the largest the tool writes is that many frames of the worst case,
// which is a picture of 255 random colours: a frame of N pixels is under 1.5 N
// bytes, and that many of them are under the 8 MiB the client takes.
test("the largest recording the tool allows is under the 8 MiB the client takes", async () => {
  const { MAX_GIF_FRAMES, MAX_GIF_SIDE } = await import("../screen.js");
  const side = MAX_GIF_SIDE;
  const bytes = encodeGif({
    width: side,
    height: side,
    frames: [picture(side, side, 255, 9)],
    delays: [5],
  });
  const perFrame = bytes.length - 13 - 768 - 19 - 1 - 8 - 10;
  assert.ok(perFrame < 1.5 * side * side, `${perFrame} bytes for ${side * side} pixels`);
  assert.ok(MAX_GIF_FRAMES * bytes.length < 8 * 1024 * 1024, `${MAX_GIF_FRAMES} frames`);
});

// A GIF encoder for record_gif: frames of RGBA pixels in, the bytes of an
// animated GIF out. It imports nothing and calls no browser API.
//
// Its limits, which the tool's description and BROWSER-42 state:
//   - one palette of at most 256 colours for the whole recording, made by median
//     cut over the colours of every frame, each channel counted to 5 bits, so a
//     smooth gradient shows bands;
//   - no dithering, no transparency (the alpha of a pixel is ignored), no local
//     palettes, and every frame is written whole, so a still page costs as much
//     as a busy one;
//   - a width and height of at most 65,535, which the format can hold;
//   - the recording repeats for ever.

// The most colours a GIF palette holds.
export const MAX_COLORS = 256;

// The size of a GIF the tool returns is bounded by the frames it asks for, not
// by anything here: a frame of N pixels encodes to under 1.5 N bytes.

// The number of bits of each channel the colours are counted to.
const KEY_BITS = 5;
const KEYS = 1 << (3 * KEY_BITS);
const SHIFT = 8 - KEY_BITS;

// A channel value of 5 bits as one of 8, with its top bits repeated so that 31
// becomes 255.
const widen = (value) => (value << SHIFT) | (value >> (KEY_BITS - SHIFT));

// The colour counted for a pixel: 5 bits each of red, green and blue.
function keyOf(rgba, at) {
  return (
    ((rgba[at] >> SHIFT) << (2 * KEY_BITS)) |
    ((rgba[at + 1] >> SHIFT) << KEY_BITS) |
    (rgba[at + 2] >> SHIFT)
  );
}

// How many pixels of all the frames have each colour.
function histogramOf(frames) {
  const counts = new Uint32Array(KEYS);
  for (const rgba of frames) {
    for (let at = 0; at < rgba.length; at += 4) {
      counts[keyOf(rgba, at)] += 1;
    }
  }
  return counts;
}

// The channel (0 red, 1 green, 2 blue) of a colour key.
const channelOf = (key, axis) =>
  (key >> ((2 - axis) * KEY_BITS)) & ((1 << KEY_BITS) - 1);

// Splits the colours that occur into at most MAX_COLORS boxes by median cut:
// the box holding the most pixels that has more than one colour is cut across
// its widest channel at the colour where half its pixels are passed. Gives the
// palette (the weighted mean of each box, as 8-bit red, green and blue, padded
// with black to 256 entries) and, for each colour key that occurs, the index of
// its box.
export function paletteOf(counts) {
  const keys = [];
  for (let key = 0; key < KEYS; key += 1) {
    if (counts[key] > 0) {
      keys.push(key);
    }
  }
  const population = (list) => list.reduce((sum, key) => sum + counts[key], 0);
  let boxes = [{ keys, pixels: population(keys) }];
  while (boxes.length < MAX_COLORS) {
    let cut = -1;
    boxes.forEach((box, at) => {
      if (box.keys.length > 1 && (cut < 0 || box.pixels > boxes[cut].pixels)) {
        cut = at;
      }
    });
    if (cut < 0) {
      break;
    }
    const { keys: list, pixels } = boxes[cut];
    let axis = 0;
    let widest = -1;
    for (let candidate = 0; candidate < 3; candidate += 1) {
      const values = list.map((key) => channelOf(key, candidate));
      const width = Math.max(...values) - Math.min(...values);
      if (width > widest) {
        widest = width;
        axis = candidate;
      }
    }
    list.sort(
      (a, b) => channelOf(a, axis) - channelOf(b, axis) || a - b,
    );
    let passed = 0;
    let at = 0;
    while (at < list.length - 2 && passed + counts[list[at]] < pixels / 2) {
      passed += counts[list[at]];
      at += 1;
    }
    const first = list.slice(0, at + 1);
    const second = list.slice(at + 1);
    boxes = [
      ...boxes.slice(0, cut),
      { keys: first, pixels: population(first) },
      { keys: second, pixels: population(second) },
      ...boxes.slice(cut + 1),
    ];
  }
  const palette = new Uint8Array(MAX_COLORS * 3);
  const lookup = new Uint8Array(KEYS);
  boxes.forEach((box, index) => {
    const sums = [0, 0, 0];
    for (const key of box.keys) {
      lookup[key] = index;
      for (let axis = 0; axis < 3; axis += 1) {
        sums[axis] += channelOf(key, axis) * counts[key];
      }
    }
    for (let axis = 0; axis < 3; axis += 1) {
      palette[index * 3 + axis] = widen(Math.round(sums[axis] / box.pixels));
    }
  });
  return { palette, lookup };
}

// The code that clears the table, the one that ends the data, and the largest
// number of codes a table holds.
const CLEAR = 256;
const END = 257;
const TABLE_SIZE = 4096;

// The LZW data of a frame's palette indices, as a GIF stores them for a minimum
// code size of 8: codes of 9 to 12 bits, least significant bit first, the table
// cleared when it is full. Gives the bytes, not yet cut into sub-blocks.
export function lzw(indices) {
  const bytes = [];
  let held = 0;
  let bits = 0;
  const put = (code, size) => {
    held |= code << bits;
    bits += size;
    while (bits >= 8) {
      bytes.push(held & 0xff);
      held >>>= 8;
      bits -= 8;
    }
  };
  const table = new Map();
  let size = 9;
  let free = END + 1;
  put(CLEAR, size);
  let current = indices[0];
  for (let at = 1; at < indices.length; at += 1) {
    const next = indices[at];
    const key = current * 256 + next;
    const found = table.get(key);
    if (found !== undefined) {
      current = found;
      continue;
    }
    put(current, size);
    if (free < TABLE_SIZE) {
      table.set(key, free);
      free += 1;
      if (free > 1 << size) {
        size += 1;
      }
    } else {
      put(CLEAR, size);
      table.clear();
      free = END + 1;
      size = 9;
    }
    current = next;
  }
  put(current, size);
  // A decoder that has just filled its table reads the end code with one more
  // bit than this writes. The code is 257, so that bit is 0, and the data up to
  // here is a whole number of bytes at every such point (2304, 7424 and 18688
  // bits), so the bit is the padding of the last byte.
  put(END, size);
  if (bits > 0) {
    bytes.push(held & 0xff);
  }
  return bytes;
}

const word = (value) => [value & 0xff, (value >> 8) & 0xff];

const ascii = (text) => [...text].map((character) => character.charCodeAt(0));

// The most a sub-block of data holds.
const BLOCK = 255;

// The bytes of an animated GIF.
//
//   width, height   the size of every frame, in pixels
//   frames          the frames, each width * height * 4 bytes of red, green,
//                   blue and alpha, which is ignored
//   delays          how long each frame stays, in hundredths of a second
export function encodeGif({ width, height, frames, delays }) {
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > 0xffff ||
    height > 0xffff
  ) {
    throw new RangeError("a GIF is 1 to 65535 pixels wide and high");
  }
  if (frames.length === 0 || delays.length !== frames.length) {
    throw new RangeError("a GIF needs a frame, and a delay for each");
  }
  for (const rgba of frames) {
    if (rgba.length !== width * height * 4) {
      throw new RangeError("a frame is not as large as the picture");
    }
  }
  const { palette, lookup } = paletteOf(histogramOf(frames));
  const out = [
    ...ascii("GIF89a"),
    ...word(width),
    ...word(height),
    // A global palette of 256 colours, 8 bits of colour, not sorted.
    0xf7,
    0,
    0,
    ...palette,
    // The extension that makes the animation repeat for ever.
    0x21,
    0xff,
    0x0b,
    ...ascii("NETSCAPE2.0"),
    3,
    1,
    0,
    0,
    0,
  ];
  frames.forEach((rgba, number) => {
    const indices = new Uint8Array(width * height);
    for (let pixel = 0; pixel < indices.length; pixel += 1) {
      indices[pixel] = lookup[keyOf(rgba, pixel * 4)];
    }
    const delay = Math.min(0xffff, Math.max(0, Math.round(delays[number])));
    out.push(
      // Graphic control: leave the frame in place, then the delay.
      0x21,
      0xf9,
      4,
      0x04,
      ...word(delay),
      0,
      0,
      // The frame fills the picture and uses the global palette.
      0x2c,
      ...word(0),
      ...word(0),
      ...word(width),
      ...word(height),
      0,
      8,
    );
    const data = lzw(indices);
    for (let at = 0; at < data.length; at += BLOCK) {
      const block = data.slice(at, at + BLOCK);
      out.push(block.length, ...block);
    }
    out.push(0);
  });
  out.push(0x3b);
  return Uint8Array.from(out);
}

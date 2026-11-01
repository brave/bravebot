// A reader of GIF89a written from the format's description, for the tests of the
// encoder behind record_gif. It shares no code with gif.js.

import assert from "node:assert/strict";

// A GIF read as the format says: the header, the screen, the global palette,
// the extensions it skips or notes, and each frame's indices.
export function readGif(bytes) {
  let at = 0;
  const byte = () => bytes[at++];
  const word = () => byte() | (byte() << 8);
  const text = (count) => String.fromCharCode(...bytes.subarray(at, (at += count)));
  assert.equal(text(6), "GIF89a");
  const width = word();
  const height = word();
  const packed = byte();
  byte();
  byte();
  assert.equal(packed & 0x80, 0x80, "a global palette");
  const palette = bytes.subarray(at, (at += 3 * (2 << (packed & 7))));
  const frames = [];
  let loops;
  let delay = 0;
  for (;;) {
    const introducer = byte();
    if (introducer === 0x3b) {
      break;
    }
    if (introducer === 0x21) {
      const label = byte();
      const blocks = [];
      for (let size = byte(); size > 0; size = byte()) {
        blocks.push(bytes.subarray(at, (at += size)));
      }
      if (label === 0xf9) {
        delay = blocks[0][1] | (blocks[0][2] << 8);
      } else if (label === 0xff) {
        assert.equal(String.fromCharCode(...blocks[0]), "NETSCAPE2.0");
        loops = blocks[1][1] | (blocks[1][2] << 8);
      }
      continue;
    }
    assert.equal(introducer, 0x2c, "an image or an extension");
    const [left, top, w, h] = [word(), word(), word(), word()];
    assert.deepEqual([left, top, w, h], [0, 0, width, height]);
    assert.equal(byte(), 0, "no local palette, not interlaced");
    const minimum = byte();
    const data = [];
    for (let size = byte(); size > 0; size = byte()) {
      data.push(...bytes.subarray(at, (at += size)));
    }
    frames.push({ delay, indices: unpack(data, minimum, width * height) });
  }
  assert.equal(at, bytes.length, "nothing after the trailer");
  return { width, height, palette, frames, loops };
}

// The LZW decoder of the format's description.
export function unpack(data, minimum, count) {
  const clear = 1 << minimum;
  const end = clear + 1;
  let table;
  let size;
  const reset = () => {
    table = Array.from({ length: clear }, (_, value) => [value]);
    table.push(null, null);
    size = minimum + 1;
  };
  reset();
  const out = [];
  let held = 0;
  let bits = 0;
  let at = 0;
  let previous = null;
  for (;;) {
    while (bits < size) {
      assert.ok(at < data.length, "the data ends before the end code");
      held |= data[at++] << bits;
      bits += 8;
    }
    const code = held & ((1 << size) - 1);
    held >>>= size;
    bits -= size;
    if (code === clear) {
      reset();
      previous = null;
      continue;
    }
    if (code === end) {
      break;
    }
    let entry;
    if (code < table.length) {
      entry = table[code];
    } else {
      assert.equal(code, table.length, "a code is in the table or the next");
      entry = [...previous, previous[0]];
    }
    out.push(...entry);
    if (previous !== null && table.length < 4096) {
      table.push([...previous, entry[0]]);
    }
    if (table.length === 1 << size && size < 12) {
      size += 1;
    }
    previous = entry;
  }
  assert.equal(out.length, count);
  assert.equal(at, data.length, "the end code is the last code");
  return out;
}

// What the icon tests read of a file: a PNG's size and pixels, and the files under a directory.
import assert from 'node:assert/strict'
import { readdirSync } from 'node:fs'
import { join, relative } from 'node:path'
import { inflateSync } from 'node:zlib'

// A PNG says its size in the header chunk that follows the eight signature bytes.
export function pngSize(bytes) {
  assert.equal(bytes.subarray(1, 4).toString('latin1'), 'PNG', 'not a PNG')
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
}

// The pixels of an 8-bit RGBA PNG that is not interlaced, which is what both tools that write the
// icons here produce. Two files can hold one picture and differ in bytes, since an encoder
// chooses its own filters and compression.
export function pixels(bytes) {
  const { width, height } = pngSize(bytes)
  assert.deepEqual([...bytes.subarray(24, 29)], [8, 6, 0, 0, 0], 'not 8-bit RGBA without interlacing')
  const chunks = []
  for (let at = 8; at < bytes.length; at += 12 + bytes.readUInt32BE(at)) {
    if (bytes.subarray(at + 4, at + 8).toString('latin1') === 'IDAT') chunks.push(bytes.subarray(at + 8, at + 8 + bytes.readUInt32BE(at)))
  }
  const raw = inflateSync(Buffer.concat(chunks))
  const stride = width * 4
  const out = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)]
    for (let x = 0; x < stride; x++) {
      const left = x >= 4 ? out[y * stride + x - 4] : 0
      const up = y > 0 ? out[(y - 1) * stride + x] : 0
      const upLeft = y > 0 && x >= 4 ? out[(y - 1) * stride + x - 4] : 0
      const paeth = () => {
        const p = left + up - upLeft
        const [a, b, c] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - upLeft)]
        return a <= b && a <= c ? left : b <= c ? up : upLeft
      }
      const predictor = [0, left, up, (left + up) >> 1, paeth()][filter]
      out[y * stride + x] = (raw[y * (stride + 1) + 1 + x] + predictor) & 255
    }
  }
  return out
}

export function files(root) {
  return readdirSync(root, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(root, join(entry.parentPath, entry.name)))
    .sort()
}

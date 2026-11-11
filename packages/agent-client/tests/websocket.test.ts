import assert from 'node:assert/strict'
import { Duplex } from 'node:stream'
import { test } from 'node:test'
import { MAX_MESSAGE_BYTES, WebSocketEnd, frame, parse } from '../scripts/websocket.js'

/** A socket whose far end is the test: what the relay writes is kept, and `arrive` hands it bytes. */
class FakeSocket extends Duplex {
  written: Buffer[] = []
  override _read(): void {}
  override _write(chunk: Buffer, _: BufferEncoding, done: () => void): void {
    this.written.push(chunk)
    done()
  }
  arrive(bytes: Buffer): void {
    this.push(bytes)
  }
}

/** A frame as a client sends it: masked, with `fin` and `rsv` as given. */
function clientFrame(opcode: number, payload: Buffer, { fin = true, rsv = 0, masked = true } = {}): Buffer {
  const mask = Buffer.from([0x11, 0x22, 0x33, 0x44])
  const length =
    payload.length < 126
      ? Buffer.from([payload.length])
      : Buffer.from([126, payload.length >> 8, payload.length & 0xff])
  length[0]! |= masked ? 0x80 : 0
  const body = Buffer.from(payload)
  if (masked) for (let index = 0; index < body.length; index++) body[index]! ^= mask[index & 3]!
  return Buffer.concat([Buffer.from([(fin ? 0x80 : 0) | rsv | opcode]), length, masked ? mask : Buffer.alloc(0), body])
}

function opened(): { socket: FakeSocket; texts: string[]; ends: string[] } {
  const socket = new FakeSocket()
  const texts: string[] = []
  const ends: string[] = []
  new WebSocketEnd(socket, { onText: (text) => texts.push(text), onEnd: (detail) => ends.push(detail) })
  return { socket, texts, ends }
}

/** The close code in the last frame the relay wrote, or null when it wrote no close frame. */
function closeCode(socket: FakeSocket): number | null {
  const last = socket.written.at(-1)
  if (last === undefined || (last[0]! & 0x0f) !== 0x8) return null
  return last.readUInt16BE(2)
}

const settle = () => new Promise((resolve) => setImmediate(resolve))

test('a text message split over frames and over reads arrives once, whole', async () => {
  const { socket, texts, ends } = opened()
  const first = clientFrame(0x1, Buffer.from('hello, '), { fin: false })
  const second = clientFrame(0x0, Buffer.from('wörld'))
  const bytes = Buffer.concat([first, second])
  for (let at = 0; at < bytes.length; at += 3) socket.arrive(bytes.subarray(at, at + 3))
  await settle()
  assert.deepEqual(texts, ['hello, wörld'])
  assert.deepEqual(ends, [])
})

test('a ping is answered with a pong that carries its payload, and is not passed on', async () => {
  const { socket, texts } = opened()
  socket.arrive(clientFrame(0x9, Buffer.from('are you there')))
  await settle()
  assert.deepEqual(texts, [])
  assert.deepEqual(socket.written, [frame(0xa, Buffer.from('are you there'))])
})

for (const [what, bytes, code] of [
  ['an unmasked frame', clientFrame(0x1, Buffer.from('hi'), { masked: false }), 1002],
  ['a binary message', clientFrame(0x2, Buffer.from('hi')), 1003],
  ['a reserved bit', clientFrame(0x1, Buffer.from('hi'), { rsv: 0x40 }), 1002],
  ['a continuation with no message to continue', clientFrame(0x0, Buffer.from('hi')), 1002],
  ['a fragmented ping', clientFrame(0x9, Buffer.from('hi'), { fin: false }), 1002],
  ['text that is not UTF-8', clientFrame(0x1, Buffer.from([0xc3, 0x28])), 1007],
] as const) {
  test(`${what} closes the connection with ${code} and nothing is passed on`, async () => {
    const { socket, texts, ends } = opened()
    socket.arrive(bytes)
    await settle()
    assert.deepEqual(texts, [])
    assert.equal(closeCode(socket), code)
    assert.equal(ends.length, 1)
  })
}

test('a length over the limit is refused from the header, before the payload is waited for', async () => {
  const header = Buffer.alloc(14)
  header[0] = 0x81
  header[1] = 0x80 | 127
  header.writeBigUInt64BE(BigInt(MAX_MESSAGE_BYTES + 1), 2)
  assert.equal(parse(header), 'a frame was larger than the relay reads')
  const at = Buffer.from(header)
  at.writeBigUInt64BE(BigInt(MAX_MESSAGE_BYTES), 2)
  assert.equal(parse(at), 'incomplete', 'a frame at the limit waits for its payload')

  const { socket, ends } = opened()
  socket.arrive(header)
  await settle()
  assert.equal(closeCode(socket), 1002)
  assert.equal(ends.length, 1)
})

test('a message over the limit across frames closes with 1009', async () => {
  const { socket, texts } = opened()
  const part = Buffer.alloc(0xffff, 0x61)
  const parts = Math.ceil(MAX_MESSAGE_BYTES / part.length) + 1
  for (let index = 0; index < parts && closeCode(socket) === null; index++) {
    socket.arrive(clientFrame(index === 0 ? 0x1 : 0x0, part, { fin: false }))
    await settle()
  }
  assert.equal(closeCode(socket), 1009)
  assert.deepEqual(texts, [])
})

test('a close from the app is answered with a close and ends the connection once', async () => {
  const { socket, ends } = opened()
  const payload = Buffer.alloc(2)
  payload.writeUInt16BE(1000)
  socket.arrive(clientFrame(0x8, payload))
  socket.arrive(clientFrame(0x1, Buffer.from('after the close')))
  await settle()
  assert.equal(closeCode(socket), 1000)
  assert.equal(ends.length, 1)
})

/**
 * The server end of one WebSocket connection (RFC 6455), for the development relay.
 *
 * It reads text messages, answers pings, and closes. Binary messages, unmasked frames, reserved
 * bits, extensions and messages over the size limit end the connection, so nothing it does not
 * understand is passed on. It is written for one loopback connection from the app and has no
 * other caller.
 */

import { createHash } from 'node:crypto'
import type { IncomingMessage } from 'node:http'
import type { Duplex } from 'node:stream'

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'

/** The largest message read: the host's 8 MiB line limit, at four bytes a character, with room for its newline. */
export const MAX_MESSAGE_BYTES = 32 * 1024 * 1024 + 4

const TEXT = 0x1
const BINARY = 0x2
const CLOSE = 0x8
const PING = 0x9
const PONG = 0xa

/** The close codes this end sends. 4001 is the relay's own, for a wrong code. */
export const CLOSE_NORMAL = 1000
export const CLOSE_GOING_AWAY = 1001
export const CLOSE_PROTOCOL = 1002
export const CLOSE_UNSUPPORTED = 1003
export const CLOSE_NOT_TEXT = 1007
export const CLOSE_TOO_BIG = 1009
export const CLOSE_WRONG_CODE = 4001

export interface SocketHandlers {
  onText(text: string): void
  /** The connection ended, by either end or by a fault. Called once. */
  onEnd(detail: string): void
}

/**
 * Complete the opening handshake, or refuse it with 400 and return null. A request that is not a
 * version 13 WebSocket upgrade with a key of sixteen bytes is refused.
 */
export function accept(request: IncomingMessage, socket: Duplex, handlers: SocketHandlers): WebSocketEnd | null {
  const key = request.headers['sec-websocket-key']
  const upgrade = request.headers.upgrade
  const valid =
    request.method === 'GET' &&
    typeof upgrade === 'string' &&
    upgrade.toLowerCase() === 'websocket' &&
    request.headers['sec-websocket-version'] === '13' &&
    typeof key === 'string' &&
    Buffer.from(key, 'base64').length === 16
  if (!valid) {
    socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n')
    return null
  }
  const answer = createHash('sha1').update(key + GUID).digest('base64')
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${answer}\r\n\r\n`)
  return new WebSocketEnd(socket, handlers)
}

/** One accepted connection. */
export class WebSocketEnd {
  private buffered: Buffer = Buffer.alloc(0)
  private parts: Buffer[] = []
  private partBytes = 0
  private ended = false
  private closing = false
  private readonly decoder = new TextDecoder('utf-8', { fatal: true })

  constructor(
    private readonly socket: Duplex,
    private readonly handlers: SocketHandlers,
  ) {
    socket.on('data', (chunk: Buffer) => this.read(chunk))
    socket.on('end', () => this.end('the app closed the connection'))
    socket.on('close', () => this.end('the connection closed'))
    socket.on('error', (error: Error) => this.end(`the connection failed: ${error.message}`))
  }

  /** Send one text message. Returns false when the socket's buffer is full, as `write` does. */
  send(text: string): boolean {
    if (this.ended || this.closing) return true
    return this.socket.write(frame(TEXT, Buffer.from(text, 'utf8')))
  }

  /** Stop reading until `resume`, so a slow reader of what this end passes on is not overrun. */
  pause(): void {
    this.socket.pause()
  }

  resume(): void {
    this.socket.resume()
  }

  /** Wait until the socket's buffer has drained. */
  onDrain(callback: () => void): void {
    this.socket.once('drain', callback)
  }

  /** Send a close frame with `code` and `reason`, then end the socket. */
  close(code: number, reason: string): void {
    if (this.ended || this.closing) return
    this.closing = true
    const said = Buffer.from(reason, 'utf8').subarray(0, 123)
    const payload = Buffer.alloc(2 + said.length)
    payload.writeUInt16BE(code, 0)
    said.copy(payload, 2)
    this.socket.end(frame(CLOSE, payload))
    this.end(reason)
  }

  private end(detail: string): void {
    if (this.ended) return
    this.ended = true
    this.handlers.onEnd(detail)
    if (!this.socket.writableEnded) this.socket.end()
  }

  private read(chunk: Buffer): void {
    this.buffered = this.buffered.length === 0 ? chunk : Buffer.concat([this.buffered, chunk])
    while (!this.ended && !this.closing) {
      const parsed = parse(this.buffered)
      if (parsed === 'incomplete') return
      if (typeof parsed === 'string') return this.close(CLOSE_PROTOCOL, parsed)
      this.buffered = this.buffered.subarray(parsed.length)
      this.handle(parsed.fin, parsed.opcode, parsed.payload)
    }
  }

  private handle(fin: boolean, opcode: number, payload: Buffer): void {
    if (opcode >= CLOSE) {
      if (!fin || payload.length > 125) return this.close(CLOSE_PROTOCOL, 'a control frame was fragmented or too long')
      if (opcode === PING) this.socket.write(frame(PONG, payload))
      else if (opcode === CLOSE) this.close(CLOSE_NORMAL, 'the app closed the connection')
      else if (opcode !== PONG) this.close(CLOSE_PROTOCOL, 'an unknown control frame arrived')
      return
    }
    if (opcode === BINARY) return this.close(CLOSE_UNSUPPORTED, 'only text messages are read')
    const continuing = this.parts.length > 0
    if (opcode === 0 ? !continuing : opcode !== TEXT || continuing) {
      return this.close(CLOSE_PROTOCOL, 'a message frame arrived out of order')
    }
    this.partBytes += payload.length
    if (this.partBytes > MAX_MESSAGE_BYTES) return this.close(CLOSE_TOO_BIG, 'a message was larger than the relay reads')
    this.parts.push(payload)
    if (!fin) return
    const whole = this.parts.length === 1 ? this.parts[0]! : Buffer.concat(this.parts)
    this.parts = []
    this.partBytes = 0
    let text: string
    try {
      text = this.decoder.decode(whole)
    } catch {
      return this.close(CLOSE_NOT_TEXT, 'a message was not UTF-8 text')
    }
    this.handlers.onText(text)
  }
}

interface Frame {
  fin: boolean
  opcode: number
  payload: Buffer
  /** Bytes the frame took, header included. */
  length: number
}

/** The first frame in `data`, `incomplete` when more bytes are needed, or why the bytes are not a frame from a client. */
export function parse(data: Buffer): Frame | 'incomplete' | string {
  if (data.length < 2) return 'incomplete'
  const first = data[0]!
  const second = data[1]!
  if ((first & 0x70) !== 0) return 'a frame set a reserved bit'
  if ((second & 0x80) === 0) return 'a frame from the app was not masked'
  let size = second & 0x7f
  let at = 2
  if (size === 126) {
    if (data.length < 4) return 'incomplete'
    size = data.readUInt16BE(2)
    at = 4
  } else if (size === 127) {
    if (data.length < 10) return 'incomplete'
    const large = data.readBigUInt64BE(2)
    if (large > BigInt(MAX_MESSAGE_BYTES)) return 'a frame was larger than the relay reads'
    size = Number(large)
    at = 10
  }
  if (size > MAX_MESSAGE_BYTES) return 'a frame was larger than the relay reads'
  if (data.length < at + 4 + size) return 'incomplete'
  const mask = data.subarray(at, at + 4)
  const payload = Buffer.from(data.subarray(at + 4, at + 4 + size))
  for (let index = 0; index < payload.length; index++) payload[index]! ^= mask[index & 3]!
  return { fin: (first & 0x80) !== 0, opcode: first & 0x0f, payload, length: at + 4 + size }
}

/** An unmasked frame, as a server sends. */
export function frame(opcode: number, payload: Buffer): Buffer {
  const header =
    payload.length < 126
      ? Buffer.from([0x80 | opcode, payload.length])
      : payload.length < 0x10000
        ? Buffer.from([0x80 | opcode, 126, payload.length >> 8, payload.length & 0xff])
        : Buffer.concat([Buffer.from([0x80 | opcode, 127]), bigEndian64(payload.length)])
  return Buffer.concat([header, payload])
}

function bigEndian64(value: number): Buffer {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(value))
  return bytes
}

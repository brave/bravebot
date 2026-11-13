/**
 * The development relay: a client program for `bravebot-host` that hands its channel to one app
 * over a loopback WebSocket, for the stage-3d phone experiment.
 *
 *   bravebot-host --sessions-root <root> --workspace <dir> -- <node> dist/scripts/dev-relay.js --workspace <dir>
 *
 * Its standard input and output are the channel the host made for it. It listens on 127.0.0.1 on
 * a port the system picks and shows the port and a six-digit code made for this launch on standard
 * error. Nothing writes the code anywhere else, and it is not part of the address. It takes one
 * connection, and the first message on it must be the code: a wrong code, or none within a minute,
 * ends the relay. After the code it sends the workspaces it was given, the same `--workspace`
 * directories the host was given, and then passes text both ways without reading it. The relay
 * exits when either end closes, which ends the host's channel; typing `launch` to the host starts
 * a new relay with a new port and code.
 *
 * This is a development transport. It is reachable by any program on the computer that can reach
 * 127.0.0.1, which the code alone stands between, and it does not decide U1 or authorize control
 * of a real session. Run it with synthetic data only.
 */

import { randomInt, timingSafeEqual } from 'node:crypto'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { basename, isAbsolute } from 'node:path'
import { parseArgs } from 'node:util'
import { CLOSE_GOING_AWAY, CLOSE_PROTOCOL, CLOSE_WRONG_CODE, accept, type WebSocketEnd } from './websocket.js'

function fail(message: string): never {
  process.stderr.write(`dev-relay: ${message}\n`)
  process.exit(2)
}

/** Tests shorten the wait for the code with `BRAVEBOT_DEV_RELAY_CODE_WAIT_MS`. */
const waitSetting = process.env.BRAVEBOT_DEV_RELAY_CODE_WAIT_MS
if (waitSetting !== undefined && !/^[1-9][0-9]{0,8}$/.test(waitSetting)) fail(`BRAVEBOT_DEV_RELAY_CODE_WAIT_MS is a whole number of milliseconds above 0: ${waitSetting}`)
const CODE_WAIT_MS = waitSetting === undefined ? 60_000 : Number(waitSetting)

const { values } = parseArgs({ options: { workspace: { type: 'string', multiple: true } } })
const directories = [...new Set(values.workspace ?? [])]
if (directories.length === 0) fail('usage: dev-relay --workspace <dir> [--workspace <dir>]...')
for (const directory of directories) if (!isAbsolute(directory)) fail(`a workspace is an absolute path: ${directory}`)
const hello = JSON.stringify({ workspaces: directories.map((directory) => ({ id: directory, name: basename(directory) || directory, directory })) })

const code = String(randomInt(0, 1_000_000)).padStart(6, '0')
const host = process.stdin
const out = process.stdout
host.setEncoding('utf8')

let connection: WebSocketEnd | null = null
let admitted = false
/** What the host wrote before an app gave the code. Its input is read from the start, so that the
 * relay sees the host end even when no app has connected. */
let held: string[] = []
let exitCode = 0

function finish(detail: string): void {
  process.stderr.write(`dev-relay: ${detail}; the relay is ending\n`)
  server.close()
  out.end(() => process.exit(exitCode))
}

host.on('end', () => {
  connection?.close(CLOSE_GOING_AWAY, 'the host ended the connection')
  if (connection === null) finish('the host ended the connection')
})

const server = createServer((_, response) => {
  response.writeHead(426, { Connection: 'close' }).end()
})

server.on('upgrade', (request, socket) => {
  if (connection !== null) {
    socket.destroy()
    return
  }
  // One connection per launch, whatever its outcome.
  server.close()
  const waiting = setTimeout(() => {
    exitCode = 1
    connection?.close(CLOSE_PROTOCOL, 'no code arrived')
  }, CODE_WAIT_MS)
  const opened = accept(request, socket, {
    onText(text) {
      if (!admitted) {
        clearTimeout(waiting)
        const given = Buffer.from(text, 'utf8')
        const expected = Buffer.from(code, 'utf8')
        if (given.length !== expected.length || !timingSafeEqual(given, expected)) {
          exitCode = 1
          opened?.close(CLOSE_WRONG_CODE, 'the code is not the one the relay showed')
          return
        }
        admitted = true
        process.stderr.write('dev-relay: the app gave the code and is connected\n')
        opened!.send(hello)
        for (const chunk of held) toTheApp(chunk)
        held = []
        return
      }
      if (!out.write(text)) {
        opened!.pause()
        out.once('drain', () => opened!.resume())
      }
    },
    onEnd(detail) {
      clearTimeout(waiting)
      finish(detail)
    },
  })
  if (opened === null) {
    clearTimeout(waiting)
    exitCode = 1
    finish('a request that was not a WebSocket connection arrived')
    return
  }
  connection = opened
})

function toTheApp(chunk: string): void {
  if (!connection!.send(chunk)) {
    host.pause()
    connection!.onDrain(() => host.resume())
  }
}

host.on('data', (chunk: string) => {
  if (admitted) toTheApp(chunk)
  else held.push(chunk)
})

server.listen(0, '127.0.0.1', () => {
  const { port } = server.address() as AddressInfo
  process.stderr.write(`dev-relay: listening on 127.0.0.1 port ${port}; the code for this launch is ${code}\n`)
})

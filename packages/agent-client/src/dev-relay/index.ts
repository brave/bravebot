/**
 * A client over the development relay (`scripts/dev-relay.ts`): the stage-3d phone experiment's
 * transport, a WebSocket to 127.0.0.1 that a cable or emulator tunnel carries to the computer.
 *
 * It is kept out of the package's main entry so an app reaches it only by importing this one,
 * which a release build need not do. The WebSocket constructor and the timers are the caller's,
 * so this module has no platform of its own.
 */

import { RpcAgentClient, type Workspace } from '../common/client.js'
import { ProtocolError } from '../common/errors.js'
import type { Deadlines } from '../common/connection.js'
import type { AgentClient } from '../common/interface.js'

/** What this module uses of a WebSocket: the subset React Native's and the web's share. */
export interface RelaySocket {
  readonly readyState: number
  send(data: string): void
  close(code?: number, reason?: string): void
  onopen: (() => void) | null
  onmessage: ((event: { data: unknown }) => void) | null
  onerror: (() => void) | null
  onclose: ((event: { code: number; reason: string }) => void) | null
}

const OPEN = 1
/** The relay's close code for a code that is not the one it showed. */
const WRONG_CODE = 4001

export interface DevRelayOptions {
  port: number
  /** The six digits the relay showed for this launch. */
  code: string
  /** Opens a WebSocket to the address given. */
  open: (url: string) => RelaySocket
  deadlines?: Deadlines
  onDiagnostic?: (message: string) => void
}

export interface DevRelay {
  readonly client: AgentClient
  /** Resolves with the reason once the connection has ended, by either end. */
  readonly ended: Promise<string>
}

/** Why a connection to the relay did not open. */
export class RelayRefusedError extends Error {
  constructor(
    message: string,
    /** True when the relay said the code was wrong. That relay has ended, so the next try needs a new launch. */
    readonly wrongCode: boolean,
  ) {
    super(message)
    this.name = 'RelayRefusedError'
  }
}

function readWorkspaces(text: unknown): Workspace[] | null {
  if (typeof text !== 'string') return null
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return null
  }
  const list = typeof parsed === 'object' && parsed !== null ? (parsed as { workspaces?: unknown }).workspaces : undefined
  if (!Array.isArray(list)) return null
  const workspaces: Workspace[] = []
  for (const item of list) {
    if (typeof item !== 'object' || item === null) return null
    const { id, name, directory } = item as Record<string, unknown>
    if (typeof id !== 'string' || typeof name !== 'string' || typeof directory !== 'string') return null
    workspaces.push({ id, name, directory })
  }
  return workspaces
}

/**
 * Connect to the relay on 127.0.0.1 at `port`, give it `code`, and resolve with a client once the
 * relay has accepted the code and sent its workspaces. A port or code that cannot be right is
 * refused before anything is opened.
 */
export function connectDevRelay(options: DevRelayOptions): Promise<DevRelay> {
  const { port, code } = options
  if (!Number.isInteger(port) || port < 1 || port > 65535) return Promise.reject(new RangeError('the port is a number from 1 to 65535'))
  if (!/^[0-9]{6}$/.test(code)) return Promise.reject(new RangeError('the code is six digits'))
  return new Promise((resolve, reject) => {
    const socket = options.open(`ws://127.0.0.1:${port}`)
    let client: RpcAgentClient | null = null
    let settled = false
    let endedWith!: (reason: string) => void
    const ended = new Promise<string>((done) => (endedWith = done))

    socket.onopen = () => socket.send(code)
    socket.onerror = () => undefined
    socket.onmessage = (event) => {
      if (client !== null) {
        if (typeof event.data === 'string') client.receive(event.data)
        else client.transportClosed('the relay sent a message that was not text')
        return
      }
      const workspaces = readWorkspaces(event.data)
      if (workspaces === null) {
        settled = true
        socket.close(1002, 'the first message was not a list of workspaces')
        reject(new ProtocolError('the relay did not send its workspaces'))
        return
      }
      client = new RpcAgentClient(
        {
          write(line) {
            if (socket.readyState !== OPEN) throw new Error('the relay connection is not open')
            socket.send(line)
          },
        },
        {
          workspaces,
          deadlines: options.deadlines,
          onDiagnostic: options.onDiagnostic,
          onClosed: (detail) => {
            if (socket.readyState === OPEN) socket.close(1000, 'the client ended the connection')
            endedWith(detail)
          },
        },
      )
      settled = true
      resolve({ client, ended })
    }
    socket.onclose = (event) => {
      if (client !== null) {
        client.transportClosed(event.reason === '' ? `the relay connection closed (${event.code})` : `the relay ended the connection: ${event.reason}`)
        return
      }
      if (settled) return
      settled = true
      reject(
        event.code === WRONG_CODE
          ? new RelayRefusedError('the code was not the one the relay showed, and that relay has ended', true)
          : new RelayRefusedError(`the relay at 127.0.0.1 port ${port} could not be reached or closed the connection`, false),
      )
    }
  })
}

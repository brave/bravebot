/**
 * The wire contract of `bravebot-rpc` that this client depends on.
 *
 * The shapes mirror `crates/ui-bridge/src/view.rs`. `test-fixtures/wire-contract.json` is written
 * from the Rust types by a Rust test, and `tests/wire.test.ts` compares these constants and the
 * decoders against it, so a variant added in Rust fails a test here rather than going unnoticed.
 */

import { ProtocolError } from './errors.js'

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }

export const SESSION_VIEW_VERSION = 1
export const SESSION_VIEW_START = 'session.view.start'

export const STATUSES = [
  'awaiting_trust',
  'idle',
  'running',
  'waiting',
  'completed',
  'failed',
  'cancelled',
  'detached',
] as const
export type Status = (typeof STATUSES)[number]

export const ROW_KINDS = ['prompt', 'narration', 'quarantined', 'approval', 'reply', 'error', 'activity'] as const
export type RowKind = (typeof ROW_KINDS)[number]

/** Approval kinds the runtime marks supported. Mirrors `Pending::supported` in Rust. */
export const SUPPORTED_APPROVALS = ['confirm', 'run', 'fetch', 'ask'] as const
export type SupportedApproval = (typeof SUPPORTED_APPROVALS)[number]

/** One transcript row. `data` is the bridge's payload for the event, labels included, untouched. */
export interface Row {
  id: number
  turn: number
  kind: RowKind
  event: string | null
  data: JsonValue
  resolved: boolean
}

/** The question on screen. `kind` may be one this client cannot answer; `supported` says so. */
export interface Pending {
  row: number
  request: number
  kind: string
  supported: boolean
  data: JsonValue
}

/** The initial view (sequence 0) or one patch: replace the listed rows and every status field. */
export interface ViewUpdate {
  sequence: number
  turn: number
  status: Status
  pending: Pending | null
  rows: Row[]
}

export interface SessionViewCapability {
  version: 1
  start: typeof SESSION_VIEW_START
  scope: 'fresh_session'
  approvals: string[]
  reconnect: false
}

export interface BridgeEvent {
  event: string
  session: string | null
  data: unknown
}

export type Incoming =
  | { type: 'event'; event: BridgeEvent }
  | { type: 'response'; id: number; result: { ok: unknown } | { error: { code: string; message: string } } }

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function oneOf<T extends string>(allowed: readonly T[], value: unknown, what: string): T {
  if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
    throw new ProtocolError(`${what} is not a known value`)
  }
  return value as T
}

/** Read one line already parsed as JSON into an event or a response. */
export function decodeIncoming(value: unknown): Incoming {
  if (!isObject(value)) throw new ProtocolError('a message must be an object')
  if (typeof value.event === 'string') {
    const session = value.session
    if (session !== undefined && typeof session !== 'string') throw new ProtocolError('an event session must be a string')
    return { type: 'event', event: { event: value.event, session: session ?? null, data: value.data } }
  }
  const id = value.id
  if (!isCount(id)) throw new ProtocolError('a response needs a numeric id')
  if ('error' in value) {
    const failure = value.error
    if (!isObject(failure) || typeof failure.code !== 'string' || typeof failure.message !== 'string') {
      throw new ProtocolError('an error response needs a code and message')
    }
    return { type: 'response', id, result: { error: { code: failure.code, message: failure.message } } }
  }
  if (!('ok' in value)) throw new ProtocolError('a response carries ok or error')
  return { type: 'response', id, result: { ok: value.ok } }
}

function decodeRow(value: unknown): Row {
  if (!isObject(value)) throw new ProtocolError('a row must be an object')
  if (!isCount(value.id) || !isCount(value.turn)) throw new ProtocolError('a row needs numeric id and turn')
  if (value.event !== null && typeof value.event !== 'string') throw new ProtocolError('a row event is a string or null')
  if (typeof value.resolved !== 'boolean') throw new ProtocolError('a row needs a resolved flag')
  if (!('data' in value)) throw new ProtocolError('a row needs data')
  return {
    id: value.id,
    turn: value.turn,
    kind: oneOf(ROW_KINDS, value.kind, 'a row kind'),
    event: value.event,
    data: value.data as JsonValue,
    resolved: value.resolved,
  }
}

function decodePending(value: unknown): Pending | null {
  if (value === null) return null
  if (!isObject(value)) throw new ProtocolError('pending must be an object or null')
  if (!isCount(value.row) || !isCount(value.request)) throw new ProtocolError('pending needs numeric row and request')
  if (typeof value.kind !== 'string' || typeof value.supported !== 'boolean' || !('data' in value)) {
    throw new ProtocolError('pending needs kind, supported and data')
  }
  return { row: value.row, request: value.request, kind: value.kind, supported: value.supported, data: value.data as JsonValue }
}

/** Read `session.view.initial` or `session.view.update` data. */
export function decodeUpdate(value: unknown): ViewUpdate {
  if (!isObject(value)) throw new ProtocolError('a view update must be an object')
  if (!isCount(value.sequence) || !isCount(value.turn)) throw new ProtocolError('a view update needs numeric sequence and turn')
  if (!Array.isArray(value.rows)) throw new ProtocolError('a view update needs rows')
  return {
    sequence: value.sequence,
    turn: value.turn,
    status: oneOf(STATUSES, value.status, 'a status'),
    pending: decodePending(value.pending),
    rows: value.rows.map(decodeRow),
  }
}

/** The capability the runtime advertises, or null when it is absent or is not version 1 of this contract. */
export function readSessionViewCapability(info: unknown): SessionViewCapability | null {
  if (!isObject(info) || !isObject(info.capabilities)) return null
  const view = info.capabilities.sessionView
  if (!isObject(view)) return null
  if (view.version !== SESSION_VIEW_VERSION || view.start !== SESSION_VIEW_START || view.scope !== 'fresh_session') return null
  if (view.reconnect !== false || !Array.isArray(view.approvals)) return null
  return {
    version: 1,
    start: SESSION_VIEW_START,
    scope: 'fresh_session',
    approvals: view.approvals.filter((kind): kind is string => typeof kind === 'string'),
    reconnect: false,
  }
}

/**
 * What crosses between the desktop's main process and its prototype window.
 *
 * The prototype window is the desktop as a client of `bravebot-host` (HOSTCLIENT-1). The main
 * process holds the host's channel and the shared client; the window is told the state below and
 * asks for the actions below, and nothing else. The state is the Rust view as the client holds it,
 * with the host's own words for who may answer what. Nothing here is computed from a row's text.
 */

import type { AnswerKind, AskAnswer, HeldSession, Pending, Row, Status, ViewEnd } from '../../../packages/agent-client/src/common/index'

export type { AnswerKind, AskAnswer, HeldSession, Pending, Row, Status, ViewEnd }

/** The view of the session on screen, as the shared client holds it. */
export interface PrototypeView {
  sequence: number
  turn: number
  /** What a cancel names to stop the turn on screen. */
  target: number
  status: Status
  /** The name the person gave the host's client that acted last, or null. */
  actor: string | null
  /** The question on screen, which may be one this client may not answer (`answerable` says). */
  pending: Pending | null
  rows: readonly Row[]
  /** How many rows before the first in `rows` the view does not hold. */
  omitted: number
  /** The view ended, and why. It is not recovered. */
  ended: ViewEnd | null
}

export interface PrototypeSession {
  id: string
  view: PrototypeView
  /** The startup trust question as the host sent it, or null where none is waiting for this client. */
  startupTrust: unknown
}

/** Everything the window draws. */
export interface PrototypeState {
  /** The host's channel: `ended` once the host has gone, which this client cannot recover from. */
  channel: 'open' | 'ended'
  endedReason: string | null
  /** The name the person gave this client on the host's command line. */
  name: string
  /** The one folder this client opens sessions in, as a name to draw. */
  workspace: string
  /** What the host lets this client answer. Empty until the host has said. */
  answers: readonly AnswerKind[]
  /** The sessions the host holds, and whether this client is attached to each. */
  held: readonly HeldSession[]
  session: PrototypeSession | null
}

/** What the window may ask the main process for. */
export type PrototypeAction =
  | { kind: 'refresh' }
  | { kind: 'new' }
  | { kind: 'attach'; session: string }
  | { kind: 'leave' }
  // These name the session they are for, so one that arrives after the person has gone to another
  // is refused and not applied to it. Question numbers are each session's own, so a number alone
  // could be taken for another session's.
  | { kind: 'send'; session: string; text: string }
  | { kind: 'cancel'; session: string }
  | { kind: 'decide'; session: string; request: number; decision: 'approve' | 'reject' }
  | { kind: 'answer'; session: string; request: number; answers: AskAnswer[] }
  | { kind: 'trust'; session: string; trusted: boolean }

/** The outcome of an action. A refusal says the host's code and its words, and changes nothing here. */
export type PrototypeOutcome = { ok: true } | { ok: false; code: string; message: string }

/** The longest message the window may send, so a page cannot ask the main process to carry a book. */
export const LONGEST_MESSAGE = 100_000

const isCount = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0

const named = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length <= 128

/** One answer to a question the planner asked: declined, typed, or chosen from the offered ones. */
function isAskAnswer(value: unknown): value is AskAnswer {
  if (value === null) return true
  if (typeof value !== 'object' || Array.isArray(value)) return false
  const fields = value as Record<string, unknown>
  const keys = Object.keys(fields)
  if (keys.length !== 1) return false
  if (keys[0] === 'typed') return typeof fields.typed === 'string' && fields.typed.length <= LONGEST_MESSAGE
  return keys[0] === 'chosen' && Array.isArray(fields.chosen) && fields.chosen.length <= 64 && fields.chosen.every(isCount)
}

/**
 * The action a value from the window is, or null. Nothing is coerced: a field of another type, or a
 * field the action does not have, makes it no action at all.
 */
export function parseAction(value: unknown): PrototypeAction | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null
  const fields = value as Record<string, unknown>
  const only = (...names: string[]): boolean => Object.keys(fields).every((key) => key === 'kind' || names.includes(key))
  switch (fields.kind) {
    case 'refresh':
    case 'new':
    case 'leave':
      return only() ? ({ kind: fields.kind } as PrototypeAction) : null
    case 'attach':
      return only('session') && named(fields.session) ? { kind: 'attach', session: fields.session } : null
    case 'cancel':
      return only('session') && named(fields.session) ? { kind: 'cancel', session: fields.session } : null
    case 'send':
      return only('session', 'text') && named(fields.session) && typeof fields.text === 'string' && fields.text.trim() !== '' && fields.text.length <= LONGEST_MESSAGE
        ? { kind: 'send', session: fields.session, text: fields.text }
        : null
    case 'decide':
      return only('session', 'request', 'decision') && named(fields.session) && isCount(fields.request) && (fields.decision === 'approve' || fields.decision === 'reject')
        ? { kind: 'decide', session: fields.session, request: fields.request, decision: fields.decision }
        : null
    case 'answer':
      return only('session', 'request', 'answers') && named(fields.session) && isCount(fields.request) && Array.isArray(fields.answers) && fields.answers.length <= 64 && fields.answers.every(isAskAnswer)
        ? { kind: 'answer', session: fields.session, request: fields.request, answers: fields.answers as AskAnswer[] }
        : null
    case 'trust':
      return only('session', 'trusted') && named(fields.session) && typeof fields.trusted === 'boolean'
        ? { kind: 'trust', session: fields.session, trusted: fields.trusted }
        : null
    default:
      return null
  }
}
